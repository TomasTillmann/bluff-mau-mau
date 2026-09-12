"""Persistent, reproducible paired-seat comparisons of the baseline policies."""

import argparse
from collections import deque
from concurrent.futures import ProcessPoolExecutor, TimeoutError
from contextlib import closing, contextmanager
import csv
from datetime import datetime
import fcntl
import hashlib
import json
import math
import multiprocessing
from numbers import Number
import os
from pathlib import Path
from random import Random
import signal
import sqlite3
import sys
import tempfile
import threading
import time

import bluff_mau_mau
from . import baseline, matches, observation
from .observation import Bot


FORMAT_VERSION = 1
CSV_FIELDS = ("rank", "name", "elo", "games", "wins", "draws", "losses", "points", "score_rate")


def _integer(value, name, minimum=None):
    if type(value) is not int or minimum is not None and value < minimum:
        qualifier = "an integer" if minimum is None else ("a positive integer" if minimum else "a nonnegative integer")
        raise ValueError(f"{name} must be {qualifier}")
    return value


def baseline_roster(kind="all") -> dict[str, Bot]:
    """Return every named grid policy, or the three default policies."""
    if kind not in ("all", "basic"):
        raise ValueError("roster must be 'all' or 'basic'")
    bots = [baseline.RandomLegal(), baseline.HonestFirst()]
    bots.extend(baseline.mixed_grid() if kind == "all" else [baseline.MixedGreedy()])
    return {bot.name: bot for bot in bots}


def _seed(seed, domain):
    return int.from_bytes(hashlib.sha256(f"{domain}:{seed}".encode()).digest()[:8], "big") & ((1 << 63) - 1)


def round_pairs(names, seed, round_index) -> tuple[tuple[str, str], ...]:
    """Circle scheduling, with an independently shuffled circle for each cycle."""
    _integer(seed, "seed")
    _integer(round_index, "round_index", 0)
    try:
        names = tuple(names) if not isinstance(names, (str, bytes)) else ()
    except TypeError as error:
        raise ValueError("Provide at least two unique nonempty string names") from error
    if (len(names) < 2 or any(type(name) is not str or not name for name in names)
            or len(set(names)) != len(names)):
        raise ValueError("Provide at least two unique nonempty string names")
    circle = sorted(names)
    if len(circle) % 2:
        circle.append(None)
    cycle, offset = divmod(round_index, len(circle) - 1)
    Random(_seed(seed, f"circle:{cycle}")).shuffle(circle)
    rotating = circle[1:]
    if offset:
        rotating = rotating[-offset:] + rotating[:-offset]
    circle = circle[:1] + rotating
    pairs = [(a, b) for a, b in zip(circle[:len(circle) // 2], reversed(circle[len(circle) // 2:]))
             if a is not None and b is not None]
    Random(_seed(seed, f"pairs:{round_index}")).shuffle(pairs)
    return tuple(pairs)


def _number(value, name):
    try:
        if isinstance(value, bool) or not isinstance(value, Number):
            raise ValueError
        value = float(value)
        if not math.isfinite(value):
            raise ValueError
    except (TypeError, ValueError, OverflowError) as error:
        raise ValueError(f"{name} must be a finite number, not bool") from error
    return value


def elo_update(rating_a, rating_b, score_a, k=20) -> tuple[float, float]:
    """Update both ratings from their pre-game values, without rounding."""
    a, b = _number(rating_a, "rating_a"), _number(rating_b, "rating_b")
    score, k = _number(score_a, "score_a"), _number(k, "k")
    if score not in (0, 0.5, 1) or k <= 0:
        raise ValueError("score_a must be 0, 0.5, or 1, and k must be positive")
    difference = (b - a) / 400
    # Use only nonpositive exponents, including when subtraction overflows to inf.
    power = 10 ** -abs(difference)
    expected = power / (1 + power) if difference >= 0 else 1 / (1 + power)
    delta = k * (score - expected)
    return a + delta, b - delta


def _fingerprint():
    sources = {
        "core": Path(bluff_mau_mau.__file__),
        "matches": Path(matches.__file__),
        "observation": Path(observation.__file__),
        "arena": Path(__file__),
    }
    sources.update({f"baseline/{path.name}": path
                    for path in Path(baseline.__file__).parent.glob("*.py")})
    return {"format": FORMAT_VERSION, "python": sys.version,
            "sources": {name: hashlib.sha256(path.read_bytes()).hexdigest()
                        for name, path in sorted(sources.items())}}


def _path(value):
    try:
        return Path(value).expanduser().resolve()
    except (TypeError, ValueError, OSError) as error:
        raise ValueError("Provide a valid run directory path") from error


def create_run(parent="runs", *, roster="all", seed=0, rounds=50,
               max_decisions=1000) -> Path:
    """Validate first, then create a new run with an empty authoritative ledger."""
    bots = baseline_roster(roster)
    _integer(seed, "seed")
    _integer(rounds, "rounds", 1)
    _integer(max_decisions, "max_decisions", 1)
    parent = _path(parent)
    config = {"roster": roster, "names": list(bots), "seed": seed, "rounds": rounds,
              "max_decisions": max_decisions, "fingerprint": _fingerprint()}
    parent.mkdir(parents=True, exist_ok=True)
    while True:
        path = parent / datetime.now().strftime("%Y%m%d-%H%M%S-%f-arena")
        try:
            path.mkdir()
            break
        except FileExistsError:
            continue
    with closing(sqlite3.connect(path / "arena.sqlite3")) as connection:
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("PRAGMA synchronous=FULL")
        connection.executescript("""
            CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE players (
                name TEXT PRIMARY KEY, elo REAL NOT NULL DEFAULT 1000,
                games INTEGER NOT NULL DEFAULT 0, wins INTEGER NOT NULL DEFAULT 0,
                draws INTEGER NOT NULL DEFAULT 0, losses INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE games (
                id INTEGER PRIMARY KEY, round_index INTEGER NOT NULL,
                seat0 TEXT NOT NULL REFERENCES players(name),
                seat1 TEXT NOT NULL REFERENCES players(name),
                seed INTEGER NOT NULL, bot_seed0 INTEGER NOT NULL, bot_seed1 INTEGER NOT NULL,
                winner INTEGER CHECK(winner IN (0, 1)),
                reason TEXT NOT NULL CHECK(reason IN ('win', 'max_decisions')),
                decisions INTEGER NOT NULL
            );
        """)
        with connection:
            connection.execute("INSERT INTO config VALUES ('run', ?)", (json.dumps(config),))
            connection.executemany("INSERT INTO players(name) VALUES (?)", ((name,) for name in bots))
    (path / ".arena.lock").touch()
    return path


@contextmanager
def _connect(path, *, readonly=True):
    try:
        connection = sqlite3.connect((path / "arena.sqlite3").as_uri() + ("?mode=ro" if readonly else "?mode=rw"), uri=True)
        connection.row_factory = sqlite3.Row
        with closing(connection):
            yield connection
    except sqlite3.Error as error:
        raise RuntimeError(f"Invalid or unavailable arena database at {path}: {error}") from error


def _config(connection):
    try:
        row = connection.execute("SELECT value FROM config WHERE key='run'").fetchone()
        config = json.loads(row[0])
        if not isinstance(config, dict):
            raise ValueError
        names = list(baseline_roster(config["roster"]))
        if config["names"] != names:
            raise ValueError("Stored roster does not match the baseline roster")
        _integer(config["seed"], "stored seed")
        _integer(config["rounds"], "stored rounds", 1)
        _integer(config["max_decisions"], "stored max_decisions", 1)
        if "fingerprint" not in config:
            raise ValueError("Missing source/runtime fingerprint")
        return config
    except (TypeError, KeyError, ValueError) as error:
        raise ValueError(f"Invalid arena configuration: {error}") from error


def _validate_resume(config, rounds):
    if rounds is not None and rounds < config["rounds"]:
        raise ValueError("rounds cannot shrink the stored target")
    if config["fingerprint"] != _fingerprint():
        raise RuntimeError("Arena source, format, or Python runtime fingerprint mismatch; create a new run")


def _standings(connection):
    rows = [dict(row) for row in connection.execute("SELECT * FROM players")]
    for row in rows:
        row["points"] = row["wins"] + row["draws"] / 2
        row["score_rate"] = row["points"] / row["games"] if row["games"] else None
    rows.sort(key=lambda row: (-(row["score_rate"] if row["score_rate"] is not None else -1),
                               -row["games"], -row["wins"], row["name"]))
    for rank, row in enumerate(rows, 1):
        row["rank"] = rank
    return rows


def standings(path) -> list[dict]:
    """Read fresh standings without acquiring the coordinator's writer lock."""
    with _connect(_path(path)) as connection:
        return _standings(connection)


@contextmanager
def _writer_lock(path):
    with (path / ".arena.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise RuntimeError(f"Another arena writer is already running in {path}") from error
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def _export(path, rows):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", newline="", encoding="utf-8", dir=path,
                                         prefix=".leaderboard-", suffix=".csv", delete=False) as output:
            temporary = Path(output.name)
            writer = csv.DictWriter(output, fieldnames=CSV_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path / "leaderboard.csv")
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def _print_standings(path, config, rows, top):
    games = sum(row["games"] for row in rows) // 2
    target = config["rounds"] * (len(rows) // 2) * 2
    print(f"\nRun: {path}\nGames: {games}/{target} | paired rounds target: {config['rounds']}", flush=True)
    print(f"{'#':>4}  {'Name':<30} {'Score':>7} {'Points':>8} {'Games':>7} {'W':>6} {'D':>6} {'L':>6} {'Elo':>9}")
    for row in rows[:top]:
        rate = f"{row['score_rate']:.1%}" if row["score_rate"] is not None else "-"
        print(f"{row['rank']:>4}  {row['name']:<30} {rate:>7} {row['points']:>8.1f} "
              f"{row['games']:>7} {row['wins']:>6} {row['draws']:>6} {row['losses']:>6} {row['elo']:>9.2f}")
    print("Baseline comparison: early scores and differing opponent samples are noisy.", flush=True)


def _jobs(config, start, end):
    per_round = len(config["names"]) // 2
    current_round, pairs = None, None
    game_offset = _seed(config["seed"], "games")
    for pair_id in range(start, end):
        round_index, within_round = divmod(pair_id, per_round)
        if current_round != round_index:
            pairs = round_pairs(config["names"], config["seed"], round_index)
            current_round = round_index
        # Addition guarantees distinct game seeds within the 63-bit ledger range.
        game_seed = (game_offset + pair_id) % (1 << 63)
        bot_seeds = (_seed(config["seed"], f"bot0:{pair_id}"), _seed(config["seed"], f"bot1:{pair_id}"))
        yield pair_id, round_index, pairs[within_round], game_seed, bot_seeds


def _worker_init():
    # Ctrl-C belongs to the coordinator; SIGTERM still lets the pool stop failed workers.
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    signal.signal(signal.SIGTERM, signal.SIG_DFL)
    parent = multiprocessing.parent_process()
    if parent is not None:
        def exit_with_parent():
            parent.join()
            os._exit(0)
        threading.Thread(target=exit_with_parent, daemon=True).start()


def _play_pair(bots, seed, bot_seeds, max_decisions):
    results = []
    for seats in (bots, bots[::-1]):
        result = matches.run_match(seats, seed=seed, bot_seeds=bot_seeds, max_decisions=max_decisions)
        results.append((result.winner, result.decisions))
    return tuple(results)


def _commit_pair(connection, job, results):
    pair_id, round_index, pair, seed, bot_seeds = job
    with connection:
        for leg, (winner, decisions) in enumerate(results):
            seats = pair if leg == 0 else pair[::-1]
            ratings = [connection.execute("SELECT elo FROM players WHERE name=?", (name,)).fetchone()[0]
                       for name in seats]
            score = 0.5 if winner is None else float(winner == 0)
            updated = elo_update(*ratings, score)
            connection.execute("INSERT INTO games VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                               (2 * pair_id + leg, round_index, *seats, seed, *bot_seeds, winner,
                                "max_decisions" if winner is None else "win", decisions))
            for seat, name in enumerate(seats):
                connection.execute("""UPDATE players SET elo=?, games=games+1,
                                   wins=wins+?, draws=draws+?, losses=losses+? WHERE name=?""",
                                   (updated[seat], winner == seat, winner is None,
                                    winner == 1 - seat, name))


def _run_arena(path, *, workers=None, rounds=None, max_pairs=None, top=20):
    if workers is None:
        workers = min(8, os.cpu_count() or 1)
    _integer(workers, "workers", 1)
    if rounds is not None:
        _integer(rounds, "rounds", 1)
    if max_pairs is not None:
        _integer(max_pairs, "max_pairs", 0)
    _integer(top, "top", 1)
    path = _path(path)
    # Reject invalid settings and incompatible code before touching the directory.
    with _connect(path) as connection:
        config = _config(connection)
        _validate_resume(config, rounds)
    with _writer_lock(path), _connect(path, readonly=False) as connection:
        config = _config(connection)
        _validate_resume(config, rounds)
        count, first, last = connection.execute("SELECT count(*), min(id), max(id) FROM games").fetchone()
        if count % 2 or count and (first != 0 or last != count - 1):
            raise RuntimeError("Arena ledger is not a contiguous prefix of committed pairs")
        if count > config["rounds"] * (len(config["names"]) // 2) * 2:
            raise RuntimeError("Arena ledger exceeds its stored target")
        if set(row[0] for row in connection.execute("SELECT name FROM players")) != set(config["names"]):
            raise RuntimeError("Arena player records do not match the stored roster")
        connection.execute("PRAGMA synchronous=FULL")
        if rounds is not None and rounds != config["rounds"]:
            config["rounds"] = rounds
            with connection:
                connection.execute("UPDATE config SET value=? WHERE key='run'", (json.dumps(config),))
        start = count // 2
        end = config["rounds"] * (len(config["names"]) // 2)
        if max_pairs is not None:
            end = min(end, start + max_pairs)
        jobs = iter(_jobs(config, start, end))
        roster = baseline_roster(config["roster"])
        stopped = False
        previous_handlers = {}
        last_report = time.monotonic()

        def stop(signum, frame):
            nonlocal stopped
            stopped = True

        def report():
            nonlocal last_report
            rows = _standings(connection)
            _export(path, rows)
            _print_standings(path, config, rows, top)
            last_report = time.monotonic()
            return rows

        if threading.current_thread() is threading.main_thread():
            for signum in (signal.SIGINT, signal.SIGTERM):
                previous_handlers[signum] = signal.signal(signum, stop)
        try:
            report()
            if workers == 1:
                for job in jobs:
                    if stopped:
                        break
                    _, _, pair, seed, bot_seeds = job
                    results = _play_pair(tuple(roster[name] for name in pair), seed, bot_seeds, config["max_decisions"])
                    _commit_pair(connection, job, results)
                    if time.monotonic() - last_report >= 5:
                        report()
            elif start < end and not stopped:
                with ProcessPoolExecutor(max_workers=workers, mp_context=multiprocessing.get_context("spawn"),
                                         initializer=_worker_init) as pool:
                    pending = deque()
                    try:
                        while True:
                            while not stopped and len(pending) < 8 * workers:
                                job = next(jobs, None)
                                if job is None:
                                    break
                                _, _, pair, seed, bot_seeds = job
                                future = pool.submit(_play_pair, tuple(roster[name] for name in pair),
                                                     seed, bot_seeds, config["max_decisions"])
                                pending.append((job, future))
                            if not pending:
                                break
                            job, future = pending[0]
                            try:
                                results = future.result(timeout=0.5)
                            except TimeoutError:
                                if not future.done():
                                    if time.monotonic() - last_report >= 5:
                                        report()
                                    continue
                                results = future.result()
                            _commit_pair(connection, job, results)
                            pending.popleft()
                            if stopped:
                                break
                            if time.monotonic() - last_report >= 5:
                                report()
                    finally:
                        for _, future in pending:
                            future.cancel()
        except Exception as error:
            raise RuntimeError(f"Arena stopped; uncommitted pairs remain pending: {error}") from error
        finally:
            try:
                rows = report()
                if stopped:
                    print("Stopped cleanly; resume this run to continue.", flush=True)
            finally:
                for signum, handler in previous_handlers.items():
                    signal.signal(signum, handler)
        return rows


def run_arena(path, *, workers=None, rounds=None, max_pairs=None) -> list[dict]:
    """Continue a run in schedule order; max_pairs bounds newly committed work."""
    return _run_arena(path, workers=workers, rounds=rounds, max_pairs=max_pairs)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--resume", type=Path)
    mode.add_argument("--status", type=Path)
    parser.add_argument("--runs-dir", type=Path)
    parser.add_argument("--roster", choices=("all", "basic"))
    parser.add_argument("--seed", type=int)
    parser.add_argument("--rounds", type=int)
    parser.add_argument("--max-decisions", type=int)
    parser.add_argument("--workers", type=int)
    parser.add_argument("--top", type=int, default=20)
    args = parser.parse_args(argv)
    try:
        _integer(args.top, "top", 1)
        for key in ("workers", "rounds", "max_decisions"):
            value = getattr(args, key)
            if value is not None:
                _integer(value, key, 1)
        if args.resume is not None or args.status is not None:
            if any(getattr(args, key) is not None for key in ("roster", "seed", "max_decisions", "runs_dir")):
                raise ValueError("Stored run settings cannot be overridden; only workers, top, and rounds may change on resume")
        if args.status is not None:
            if args.workers is not None or args.rounds is not None:
                raise ValueError("--status accepts --top, not --workers or --rounds")
            path = _path(args.status)
            with _connect(path) as connection:
                _print_standings(path, _config(connection), _standings(connection), args.top)
        else:
            path = args.resume
            if path is None:
                path = create_run(args.runs_dir if args.runs_dir is not None else "runs",
                                  roster=args.roster if args.roster is not None else "all",
                                  seed=args.seed if args.seed is not None else 0,
                                  rounds=args.rounds if args.rounds is not None else 50,
                                  max_decisions=args.max_decisions if args.max_decisions is not None else 1000)
            _run_arena(path, workers=args.workers, rounds=args.rounds, top=args.top)
    except (ValueError, RuntimeError, OSError) as error:
        print(f"arena: error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
