const ranks = ['7', '8', '9', '10', 'J', 'Q', 'K', 'A'];
const suits = { H: 'hearts', D: 'diamonds', C: 'clubs', S: 'spades' };
const rankNames = { J: 'Jack', Q: 'Queen', K: 'King', A: 'Ace' };
const backPath = '/free-playing-cards/svg cards/card backs/card back red.svg';
const game = document.querySelector('#game');
const announcement = document.querySelector('#announcement');
const debugMode = new URLSearchParams(location.search).get('debug') === '1';
let bots = [];
let botQuery = '';
let visibleBots = 40;
let requestedBot = null;
let state = null;
let actual = null;
let declared = null;
let chosenSuit = null;
let busy = false;
let renderedState = null;

const escapeHTML = text => String(text).replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[char]));
const rank = card => card.slice(0, -1);
const suit = card => card.slice(-1);
const cardName = card => `${rankNames[rank(card)] || rank(card)} of ${suits[suit(card)]}`;
const cardPath = card => `/free-playing-cards/svg cards/card fronts/${suits[suit(card)]}/${(rankNames[rank(card)] || rank(card)).toLowerCase()} of ${suits[suit(card)]}.svg`;
const suitMark = code => `<svg class="bp-suit${'HD'.includes(code) ? ' bp-red' : ''}" viewBox="0 0 24 24" aria-hidden="true"><use href="#suit-${suits[code]}"/></svg>`;
const cardMark = card => `<span role="img" aria-label="${cardName(card)}">${rank(card)}${suitMark(suit(card))}</span>`;
const cardImage = (card, name = '') => `<img src="${card ? cardPath(card) : backPath}" alt="${escapeHTML(name)}" width="1500" height="2100" draggable="false">`;
const playerName = player => debugMode ? `Player ${player + 1}` : player === 0 ? 'You' : state?.bot?.name || 'Opponent';
const pluralCards = count => `${count} ${count === 1 ? 'card' : 'cards'}`;
const handCount = player => state.hand_counts?.[player] ?? state.hands[player].length;
const endpoint = action => `/api/${debugMode ? '' : 'play/'}${action}`;
const number = value => Number(value).toLocaleString('en-US');
const percentage = value => `${(Number(value) * 100).toFixed(1)}%`;
const ratingKind = bot => bot.elo_kind === 'performance' ? 'test Elo' : 'arena Elo';

function fuzzyScore(bot, query) {
  const normalize = text => text.normalize('NFKD').replace(/[\u0300-\u036f]/g, '').toLowerCase();
  const text = normalize(`${bot.name} ${bot.family} ${bot.description}`);
  const words = text.match(/[a-z0-9]+/g) || [];
  const tokens = normalize(query).match(/[a-z]+\d*|\d+/g) || [];
  let score = 0;
  for (const token of tokens) {
    if (/^[bnc]\d+$/.test(token)) {
      if (!words.includes(token)) return -1;
      continue;
    }
    if (text.includes(token)) { score += text.indexOf(token) / 1000; continue; }
    if (token.length < 3) return -1;
    let best = Infinity;
    for (const word of words) {
      let index = 0;
      for (const letter of word) if (letter === token[index]) index++;
      if (index === token.length) best = Math.min(best, 1 + (word.length - token.length) / 100);
      // A small edit distance also accepts substitutions and transposed letters.
      if (token.length >= 4 && Math.abs(word.length - token.length) <= 2) {
        let row = Array.from({ length: word.length + 1 }, (_, i) => i);
        for (let i = 1; i <= token.length; i++) {
          const next = [i];
          for (let j = 1; j <= word.length; j++) next[j] = Math.min(next[j - 1] + 1, row[j] + 1, row[j - 1] + Number(token[i - 1] !== word[j - 1]));
          row = next;
        }
        if (row[word.length] <= (token.length > 5 ? 2 : 1)) best = Math.min(best, 2 + row[word.length]);
      }
    }
    if (!Number.isFinite(best)) return -1;
    score += best;
  }
  return score;
}

function updateHeader() {
  document.querySelector('#debug-status').hidden = !debugMode;
  document.querySelector('#new-game').hidden = !debugMode && !state;
  document.querySelector('#choose-bot').hidden = debugMode || !state;
  const link = document.querySelector('#debug-link');
  link.hidden = !debugMode && !!state;
  link.href = debugMode ? '/' : '?debug=1';
  link.textContent = debugMode ? 'Play a bot' : 'Debug table';
  document.title = `Blafovací Prší — ${debugMode ? 'Debug table' : state ? 'You vs ' + state.bot.name : 'Choose an opponent'}`;
}

function renderBotResults() {
  const matched = bots.map(bot => ({ bot, score: fuzzyScore(bot, botQuery) }))
    .filter(item => item.score >= 0).sort((a, b) => a.score - b.score || b.bot.elo - a.bot.elo || a.bot.name.localeCompare(b.bot.name));
  document.querySelector('#bot-count').textContent = `${number(matched.length)} ${matched.length === 1 ? 'opponent' : 'opponents'}${botQuery ? ' found' : ' available'}`;
  document.querySelector('#bot-results').innerHTML = matched.length ? matched.slice(0, visibleBots).map(({ bot }) => `
    <li class="bot-row">
      <div class="bot-identity"><strong>${escapeHTML(bot.name)}</strong><p>${escapeHTML(bot.description)}</p></div>
      <div class="bot-rating"><strong>${Math.round(bot.elo)}</strong><span>${ratingKind(bot)}</span></div>
      <div class="bot-score"><strong>${percentage(bot.wins / bot.games)}</strong><span>win rate</span></div>
      <div class="bot-record"><span>${number(bot.wins)} / ${number(bot.draws)} / ${number(bot.losses)}</span><span>W / D / L · ${number(bot.games)} games</span></div>
      <button type="button" class="bp-button bp-button--primary bot-play" data-bot-id="${escapeHTML(bot.id)}" aria-label="Play against ${escapeHTML(bot.name)}">Play <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14m-6-6 6 6-6 6"/></svg></button>
    </li>`).join('') : '<li class="bot-empty"><h3>No matching opponents</h3><p>Try a shorter name or different parameters.</p><button id="clear-search" class="bp-button bp-button--secondary" type="button">Clear search</button></li>';
  const more = document.querySelector('#more-bots');
  more.hidden = matched.length <= visibleBots;
  more.textContent = `Show ${Math.min(40, Math.max(0, matched.length - visibleBots))} more`;
}

function showBots(focusSearch = false) {
  state = null;
  renderedState = null;
  requestedBot = null;
  actual = declared = chosenSuit = null;
  game.className = 'bot-lobby';
  game.innerHTML = `<section class="bot-directory" aria-labelledby="bot-title">
    <div class="bot-intro"><h2 id="bot-title">Choose your opponent</h2><p>A head-to-head game of Blafovací Prší. Your hand stays private.</p></div>
    <div class="bot-search"><label for="bot-search">Find a bot</label><div class="bot-search-field"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></svg><input id="bot-search" type="search" placeholder="Name or parameters, e.g. honest or B0 N100 C40" autocomplete="off" spellcheck="false" aria-describedby="search-help"></div><p id="search-help">Search names and strategies. Typos are okay. Press Enter to play the first match.</p></div>
    <div class="bot-list-heading"><p id="bot-count" role="status" aria-live="polite"></p><p>Ordered by rating within search matches</p></div>
    <ul id="bot-results" class="bot-results" aria-label="Available opponents"></ul>
    <button id="more-bots" type="button" class="bp-button bp-button--secondary bot-more" hidden>Show more</button>
    <p class="bot-rating-note">Arena Elo comes from the saved baseline tournament. Test Elo comes from a separate top-ten benchmark. Records are from 13 September 2026; these games do not change the ratings.</p>
  </section>`;
  document.querySelector('#bot-search').value = botQuery;
  renderBotResults();
  game.setAttribute('aria-busy', 'false');
  updateHeader();
  if (focusSearch) document.querySelector('#bot-search').focus();
}

async function loadBots() {
  busy = true;
  document.querySelector('#error').hidden = true;
  game.className = 'bot-lobby';
  game.innerHTML = '<p class="game-loading">Loading opponents…</p>';
  game.setAttribute('aria-busy', 'true');
  try {
    const response = await fetch('/api/bots', { signal: AbortSignal.timeout(10000) });
    const catalog = await response.json();
    if (!response.ok) throw new Error(catalog.error || 'Could not load opponents.');
    bots = catalog.bots;
    showBots();
  } catch (failure) {
    game.innerHTML = '<p class="game-loading">Opponents could not be loaded.</p>';
    const error = document.querySelector('#error');
    error.hidden = false;
    error.innerHTML = `<span>${escapeHTML(failure.message)}</span><button id="retry" type="button" class="bp-button bp-button--secondary bp-button--compact">Retry</button>`;
  } finally { busy = false; game.setAttribute('aria-busy', 'false'); }
}

function hand(player) {
  if (!debugMode && player !== 0) {
    const count = handCount(player);
    const fan = Array.from({ length: count }, (_, index) => {
      const position = count === 1 ? 0 : index / (count - 1) * 2 - 1;
      return `<div class="bp-card bp-card--back" data-opponent-card style="--bp-fan-angle:${position * 7}deg;--bp-fan-drop:${position * position * 12}px">${cardImage(null)}</div>`;
    }).join('');
    return `<section class="game-seat game-opponent" aria-labelledby="player-1"><h2 id="player-1" class="bp-hand-label"><strong>${escapeHTML(playerName(player))}</strong><span class="bp-hand-label__count">${pluralCards(count)} · ${Math.round(state.bot.elo)} ${ratingKind(state.bot)}</span></h2>${count ? `<div class="game-hand-scroll" data-scroll="hand-1" tabindex="0" role="region" aria-label="Opponent hand, ${pluralCards(count)}, face down"><div class="bp-hand game-fan" style="--game-card-gaps:${Math.max(1, count - 1)}" aria-hidden="true">${fan}</div></div>` : `<p class="game-empty-hand">${state.winner === player ? 'Out — game won' : 'Empty hand'}</p>`}</section>`;
  }
  const cards = state.hands[player];
  const canSelect = state.turn === player && state.winner === null;
  const fan = cards.map((card, index) => {
    const position = cards.length === 1 ? 0 : (index / (cards.length - 1)) * 2 - 1;
    return `<button id="hand-${player}-${card}" type="button" class="bp-card" style="--bp-fan-angle:${position * 7}deg;--bp-fan-drop:${position * position * 12}px" aria-label="${cardName(card)}${canSelect ? ', select actual card' : ''}" aria-pressed="${canSelect && card === actual}" data-card="${card}" ${!canSelect || busy ? 'disabled' : ''}>${cardImage(card)}</button>`;
  }).join('');
  return `<section class="game-seat" aria-labelledby="player-${player}"><h2 id="player-${player}" class="bp-hand-label"><strong>${playerName(player)}</strong></h2>${cards.length ? `<div class="game-hand-scroll" data-scroll="hand-${player}" tabindex="0" role="region" aria-label="${playerName(player)} hand, ${pluralCards(cards.length)}"><div class="bp-hand game-fan" data-active="${canSelect}" style="--game-card-gaps:${Math.max(1, cards.length - 1)}">${fan}</div></div>` : `<p class="game-empty-hand">${state.winner === player ? 'Out — game won' : 'Empty hand'}</p>`}</section>`;
}

function drawPileAction() {
  return ['draw', 'skip'].map(type => state.legal_moves.find(move => move.type === type)).find(Boolean);
}

function actionLabel(type) {
  if (type === 'challenge') return 'Call bluff on the declaration';
  if (type === 'skip') return 'Skip turn under the ace';
  if (type === 'accept') {
    if (state.skip_pending && [0, 1].some(player => handCount(player))) return 'Accept the ace and skip this turn';
    if (state.draw_penalty && !handCount(state.turn)) return `Accept and draw ${pluralCards(state.draw_penalty)}`;
    return 'Accept declaration';
  }
  return `${state.phase === 'response' ? 'Accept and draw' : 'Draw'} ${pluralCards(state.draw_penalty || 1)} and end turn`;
}

function pile(label, count, face = null, action = null) {
  const layers = Math.min(count, 3);
  const actionName = action ? `${!count && action.type === 'draw' ? 'Recycle discards. ' : ''}${actionLabel(action.type)}` : '';
  const control = content => `<button id="pile-${action.type}" type="button" class="bp-card game-pile-action${count ? '' : ' game-pile-recycle'}" data-action="${action.type}" aria-label="${escapeHTML(actionName)}" title="${escapeHTML(actionName)}" ${busy ? 'disabled' : ''}>${content}</button>`;
  const cards = count ? Array.from({ length: layers }, (_, index) => {
    const top = index === layers - 1;
    if (top && action) return control(cardImage(face));
    return `<div class="bp-card${top && face ? '' : ' bp-card--back'}">${cardImage(top ? face : null, top ? face ? cardName(face) : 'Face-down cards' : '')}</div>`;
  }).join('') : action ? control(action.type === 'draw' ? '<span>Recycle<br>and draw</span>' : '<span>Continue</span>') : 'Empty';
  return `<figure class="bp-pile"><div class="bp-pile__cards${count ? '' : ' game-pile-empty'}">${cards}</div><figcaption><span class="bp-sr-only">${label}: </span><span class="bp-pile__count">${count}</span></figcaption></figure>`;
}

function effectsText() {
  const effects = [];
  if (state.draw_penalty) effects.push(`${pluralCards(state.draw_penalty)} to draw`);
  if (state.skip_pending) effects.push('Skip pending');
  return effects.length ? effects.join(' · ') + (state.phase === 'response' ? ' if accepted' : '') : 'No pending effects';
}

function moveExplain() {
  const result = state.move_explain;
  return `<div class="game-move-explain" aria-label="Last move">${result ? `<p class="game-move-explain__title">${escapeHTML(result.title)}</p><p class="game-move-explain__detail">${escapeHTML(result.detail)}</p>` : ''}</div>`;
}

function table() {
  const faceUp = ['Starting card', 'Revealed'].includes(state.top_status);
  return `<section class="bp-table game-table" aria-label="Card table">${hand(1)}<div class="bp-piles">${pile('Draw pile', state.deck_count, null, drawPileAction())}${pile('Discards', state.pile_count, faceUp ? state.top : null, state.legal_moves.find(move => move.type === 'challenge'))}<div class="bp-declared"><p class="bp-declared__title">${faceUp ? 'Top' : 'Declared'} ${cardMark(state.top)}</p>${state.chosen_suit ? `<p class="game-declared-suit">Now ${suitMark(state.chosen_suit)} ${suits[state.chosen_suit]}</p>` : ''}</div></div><div class="bp-effects"><span>${effectsText()}</span></div>${state.winner !== null ? `<p class="game-winner">${playerName(state.winner)} wins.</p>` : state.provisional_winner !== null ? `<p class="game-pending-win">${playerName(state.provisional_winner)} is out for now. The return attempt is still in play.</p>` : ''}<div class="game-player-result">${hand(0)}${moveExplain()}</div></section>`;
}

function canDeclare(card) {
  return state.legal_moves.some(move => move.type === 'play' && move.declared === card);
}

function declarations() {
  return `<div class="bp-field"><h3 class="bp-field__title" id="declaration-label">Declare a card</h3><p class="bp-field__help">Any card can bluff a legal declaration.</p><div class="bp-declarations" data-scroll="declarations"><div class="game-declaration-grid" role="group" aria-labelledby="declaration-label">${Object.keys(suits).map(code => ranks.map(value => `<button id="declare-${value}${code}" type="button" class="bp-declaration${'HD'.includes(code) ? ' bp-red' : ''}" data-declared="${value}${code}" aria-label="Declare ${cardName(value + code)}" aria-pressed="${declared === value + code}" ${busy || state.winner !== null || !canDeclare(value + code) ? 'disabled' : ''}><span class="game-declaration-rank" aria-hidden="true">${value}</span>${suitMark(code)}</button>`).join('')).join('')}</div></div></div>`;
}

function finalPlay(declaration = declared) {
  return state.legal_moves.find(move => move.type === 'play' && move.actual === actual && move.declared === declaration && move.chosen_suit === (declaration && rank(declaration) === 'Q' ? chosenSuit : null));
}

function playReason() {
  if (state.winner !== null) return 'This game is finished. Start a new game to play again.';
  if (!handCount(state.turn)) return 'Click the discard to call bluff, or accept the declaration.';
  if (!actual) return debugMode ? 'Choose an actual card from the active player’s hand.' : 'Choose an actual card from your hand.';
  if (!declared) return 'Choose the card identity to declare.';
  const matching = state.legal_moves.filter(move => move.type === 'play' && move.actual === actual && move.declared === declared);
  if (!matching.length) {
    if (state.skip_pending) return state.phase === 'response' ? 'Counter with another ace, or accept the declaration to skip this turn.' : 'Counter with another ace, or click the draw pile to pass.';
    if (state.draw_penalty) return state.top === 'KS' ? 'Counter this king with 7 of spades or any queen.' : 'Use a seven, a queen, or king of spades on 7 of spades.';
    return `Match ${state.chosen_suit ? suits[state.chosen_suit] : `${suits[suit(state.top)]} or ${rankNames[rank(state.top)] || rank(state.top)}`}, or declare a queen.`;
  }
  if (rank(declared) === 'Q' && !chosenSuit) return 'Choose the continuing suit before playing the queen.';
  return finalPlay() ? '' : 'This combination is not available in the current turn.';
}

function declarationEffect() {
  if (!declared) return '';
  if (rank(declared) === '7' || declared === 'KS') {
    const added = declared === 'KS' ? 4 : 2;
    const opening = state.opening_card ? (state.top === 'KS' ? 4 : rank(state.top) === '7' ? 2 : 0) : 0;
    return opening ? `Starts a ${opening + added}-card penalty if accepted.` : `Adds a ${added}-card penalty if accepted.`;
  }
  if (rank(declared) === 'A') return 'Passes a skip to the opponent if accepted.';
  if (rank(declared) === 'Q') return 'Sets the continuing suit if accepted.';
  return 'No special effect if accepted.';
}

function panel() {
  const move = finalPlay();
  const truthfulMove = finalPlay(actual);
  const acceptMove = state.legal_moves.find(item => item.type === 'accept');
  const needsSuit = (declared && rank(declared) === 'Q') || (actual && rank(actual) === 'Q' && state.legal_moves.some(item => item.type === 'play' && item.actual === actual && item.declared === actual));
  const reason = playReason();
  const responding = state.phase === 'response';
  const title = state.winner !== null ? (!debugMode && state.winner === 0 ? 'You win' : `${playerName(state.winner)} wins`) : !debugMode ? (busy ? 'Updating the table…' : responding ? 'Your response' : 'Your turn') : `${playerName(state.turn)} ${responding ? 'to respond' : 'to act'}`;
  return `<aside class="bp-action-panel game-panel" aria-labelledby="actor">
    <h2 id="actor" class="bp-action-panel__title" tabindex="-1">${title}</h2>
    <div class="game-selection">
      <div class="game-actual"><h3 class="bp-field__title">Actual card</h3>${actual ? `<div class="bp-actual"><div class="bp-card bp-card--mini">${cardImage(actual, cardName(actual))}</div><p class="bp-actual__name">${cardName(actual)}</p></div>` : '<div class="bp-actual"><p class="game-actual-empty">Choose from your hand</p></div>'}</div>
      <div class="game-queen">${needsSuit ? `<h3 class="bp-field__title" id="suit-label">Continue with</h3><div class="game-suits" role="group" aria-labelledby="suit-label">${Object.entries(suits).map(([code, name]) => `<button id="suit-${code}" type="button" class="game-suit" data-suit="${code}" aria-pressed="${chosenSuit === code}" ${busy ? 'disabled' : ''}>${suitMark(code)}<span class="bp-sr-only">${name[0].toUpperCase() + name.slice(1)}</span></button>`).join('')}</div>` : ''}</div>
    </div>
    ${declarations()}
    <button id="play-truthful" type="button" class="bp-button bp-button--secondary bp-button--wide game-truthful" ${truthfulMove ? `data-move="${truthfulMove.id}"` : ''} ${!truthfulMove || busy ? 'disabled' : ''}>Play ${actual ? `${cardMark(actual)} ` : ''}as itself</button>
    <p class="bp-play-summary">${actual && declared ? `Play ${cardMark(actual)} as ${cardMark(declared)}` : 'Choose your declaration'}</p>
    <p id="play-reason" class="game-play-reason" data-invalid="${!!declared && !!reason}">${escapeHTML(reason || declarationEffect())}</p>
    <div class="bp-actions"><button id="play-card" type="button" class="bp-button bp-button--primary bp-button--wide" ${move ? `data-move="${move.id}"` : ''} ${reason ? 'aria-describedby="play-reason"' : ''} ${!move || busy ? 'disabled' : ''}>Play</button>${acceptMove ? `<button id="move-accept" type="button" class="bp-button bp-button--secondary bp-button--wide" data-move="${acceptMove.id}" title="${escapeHTML(actionLabel('accept'))}" ${busy ? 'disabled' : ''}>Accept declaration</button>` : ''}</div>
    <section class="bp-history game-history" aria-labelledby="history-title"><h3 id="history-title" class="bp-history__title">Recent moves</h3>${state.history.length ? `<ol class="bp-history__list" reversed tabindex="0" aria-label="Recent moves">${state.history.slice(-6).reverse().map(item => `<li class="bp-history__row"><span class="bp-history__player">${item.player === null ? 'Table' : playerName(item.player)}</span><span>${escapeHTML(item.text)}</span></li>`).join('')}</ol>` : '<p class="game-history-empty">The cards are dealt. Make the first move.</p>'}</section>
  </aside>`;
}

function render(focusActor = false, focused = document.activeElement.id) {
  if (!state) return;
  const scrolls = [...document.querySelectorAll('[data-scroll]')].map(element => [element.dataset.scroll, element.scrollLeft]);
  if (renderedState !== state) {
    game.innerHTML = table() + panel();
    renderedState = state;
  } else {
    game.querySelector('.game-panel').outerHTML = panel();
    for (const button of game.querySelectorAll('[data-card]')) {
      const canSelect = state.winner === null && state.hands[state.turn].includes(button.dataset.card);
      button.disabled = !canSelect || busy;
      button.setAttribute('aria-pressed', String(canSelect && button.dataset.card === actual));
    }
    for (const button of game.querySelectorAll('.game-pile-action')) button.disabled = busy;
  }
  game.setAttribute('aria-busy', String(busy));
  document.querySelector('#new-game').disabled = busy;
  for (const [key, left] of scrolls) document.querySelector(`[data-scroll="${key}"]`)?.scrollTo({ left });
  const target = focusActor ? document.querySelector('#actor') : document.getElementById(focused);
  if (target && !target.disabled) target.focus({ preventScroll: true });
}

function receive(next) {
  state = next;
  actual = null;
  declared = null;
  chosenSuit = null;
  announcement.textContent = state.move_explain ? `${state.move_explain.title} ${state.move_explain.detail}` : state.winner !== null ? `${playerName(state.winner)} wins.` : `${playerName(state.turn)} ${state.phase === 'response' ? 'can respond' : 'to act'}. ${effectsText()}.`;
}

function select(intent) {
  if (intent.card && state.hands[state.turn].includes(intent.card)) {
    if (intent.card !== actual && (!declared || rank(declared) !== 'Q')) chosenSuit = null;
    actual = intent.card;
  }
  else if (intent.declared) {
    if (!canDeclare(intent.declared)) return false;
    declared = intent.declared;
    chosenSuit = null;
  } else if (intent.suit) chosenSuit = intent.suit;
  else return false;
  announcement.textContent = !declared && finalPlay(actual) ? `${cardName(actual)} selected. Ready to play as itself.` : !declared && actual && rank(actual) === 'Q' && state.legal_moves.some(item => item.type === 'play' && item.actual === actual && item.declared === actual) ? 'Choose the continuing suit before playing the queen.' : playReason() || `${actual ? cardName(actual) : 'Card'} selected as ${declared ? cardName(declared) : 'no declaration'}. Ready to play.`;
  return true;
}

async function request(path, body) {
  if (busy) return;
  const focused = document.activeElement.id;
  busy = true;
  const error = document.querySelector('#error');
  error.hidden = true;
  game.setAttribute('aria-busy', 'true');
  document.querySelector('#new-game').disabled = true;
  render();
  announcement.textContent = body ? 'Updating the table…' : 'Loading the table…';
  let success = false;
  try {
    const response = await fetch(path, { method: body ? 'POST' : 'GET', headers: body ? { 'Content-Type': 'application/json' } : {}, body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(10000) });
    const next = await response.json();
    if (!response.ok) throw new Error(next.error || 'The table could not be updated.');
    receive(next);
    game.className = 'game-layout';
    updateHeader();
    success = true;
  } catch (failure) {
    error.innerHTML = `<span>${escapeHTML(failure.name === 'TimeoutError' ? 'The table took too long to respond. Refresh its state before trying again.' : failure.message)}</span><button id="retry" type="button" class="bp-button bp-button--secondary bp-button--compact">Refresh table</button>`;
    error.hidden = false;
    if (!state) game.innerHTML = '<p class="game-loading">The table is unavailable. Use Refresh table to reconnect.</p>';
  } finally {
    busy = false;
    game.setAttribute('aria-busy', 'false');
    document.querySelector('#new-game').disabled = false;
    render(success && (!!body || focused === 'retry'), focused);
    if (!state) document.querySelector('#retry')?.focus();
  }
}

document.addEventListener('click', event => {
  const button = event.target.closest('button');
  if (!button || button.disabled || busy) return;
  if (button.dataset?.botId) {
    requestedBot = button.dataset.botId;
    return void request('/api/play/new', { bot_id: requestedBot });
  }
  if (button.id === 'choose-bot') { document.querySelector('#error').hidden = true; return showBots(true); }
  if (button.id === 'more-bots') { visibleBots += 40; return renderBotResults(); }
  if (button.id === 'clear-search') { botQuery = ''; visibleBots = 40; showBots(true); return; }
  if (button.id === 'new-game') return void request(endpoint('new'), debugMode ? {} : { bot_id: state.bot.id });
  if (button.id === 'retry') {
    if (!debugMode && !state && !requestedBot) return void loadBots();
    return void request(state || requestedBot ? endpoint('state') : endpoint('new'), state || requestedBot ? undefined : {});
  }
  if (!state || state.winner !== null) return;
  if (button.dataset.move !== undefined) {
    const move = state.legal_moves.find(item => item.id === Number(button.dataset.move));
    if (move && (move.type !== 'play' || move === finalPlay(button.id === 'play-truthful' ? actual : declared))) return void request(endpoint('move'), { version: state.version, move_id: move.id });
    return;
  }
  const intent = button.dataset;
  if (!intent.card && !intent.declared && !intent.suit && !intent.action) return;
  if (intent.card && !state.hands[state.turn].includes(intent.card)) return;
  const move = state.legal_moves.find(item => item.type === intent.action);
  if (move) return void request(endpoint('move'), { version: state.version, move_id: move.id });
  if (select(intent)) render();
});

document.addEventListener('input', event => {
  if (event.target.id !== 'bot-search' || busy) return;
  botQuery = event.target.value;
  visibleBots = 40;
  renderBotResults();
});
document.addEventListener('keydown', event => {
  if (event.target.id !== 'bot-search' || busy || !['Enter', 'ArrowDown'].includes(event.key)) return;
  const first = document.querySelector('[data-bot-id]');
  if (!first) return;
  event.preventDefault();
  if (event.key === 'Enter') first.click();
  else first.focus();
});

function start() {
  updateHeader();
  if (debugMode) {
    game.className = 'game-layout';
    game.innerHTML = '<p class="game-loading">Dealing the cards…</p>';
    request('/api/new', {});
  } else loadBots();
}

start();
