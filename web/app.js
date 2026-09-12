const ranks = ['7', '8', '9', '10', 'J', 'Q', 'K', 'A'];
const suits = { H: 'hearts', D: 'diamonds', C: 'clubs', S: 'spades' };
const rankNames = { J: 'Jack', Q: 'Queen', K: 'King', A: 'Ace' };
const backPath = '/free-playing-cards/svg cards/card backs/card back red.svg';
const game = document.querySelector('#game');
const announcement = document.querySelector('#announcement');
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
const playerName = player => `Player ${player + 1}`;
const pluralCards = count => `${count} ${count === 1 ? 'card' : 'cards'}`;

function hand(player) {
  const cards = state.hands[player];
  const canSelect = state.turn === player && state.winner === null;
  const fan = cards.map((card, index) => {
    const position = cards.length === 1 ? 0 : (index / (cards.length - 1)) * 2 - 1;
    return `<button id="hand-${player}-${card}" type="button" class="bp-card" style="--bp-fan-angle:${position * 7}deg;--bp-fan-drop:${position * position * 12}px" aria-label="${cardName(card)}${canSelect ? state.phase === 'response' ? ', accept declaration and select actual card' : ', select actual card' : ''}" aria-pressed="${canSelect && card === actual}" data-card="${card}" ${!canSelect || busy ? 'disabled' : ''}>${cardImage(card)}</button>`;
  }).join('');
  return `<section class="game-seat" aria-labelledby="player-${player}"><h2 id="player-${player}" class="bp-hand-label"><strong>${playerName(player)}</strong></h2>${cards.length ? `<div class="game-hand-scroll" data-scroll="hand-${player}" tabindex="0" role="region" aria-label="${playerName(player)} hand, ${pluralCards(cards.length)}"><div class="bp-hand game-fan" data-active="${canSelect}" style="--game-card-gaps:${Math.max(1, cards.length - 1)}">${fan}</div></div>` : `<p class="game-empty-hand">${state.winner === player ? 'Out — game won' : 'Empty hand'}</p>`}</section>`;
}

function turnActions() {
  return state.phase === 'response' ? (state.after_accept_actions || []).map(type => ({ type })) : state.legal_moves.filter(move => ['draw', 'skip'].includes(move.type));
}

function actionLabel(type) {
  const labels = { accept: 'Accept declaration', challenge: 'Challenge declaration', skip: 'Skip turn', draw: state.draw_penalty ? `Draw ${state.draw_penalty} and end turn` : 'Draw 1 and end turn' };
  return `${state.phase === 'response' && ['draw', 'skip'].includes(type) ? 'Accept, then ' : ''}${labels[type]}`;
}

function pile(label, count, face = null, action = null) {
  const layers = Math.min(count, 3);
  const actionName = action ? `${count ? '' : 'Recycle discards. '}${actionLabel(action.type)}` : '';
  const control = content => `<button id="pile-${action.type}" type="button" class="bp-card game-pile-action${count ? '' : ' game-pile-recycle'}" data-action="${action.type}" aria-label="${escapeHTML(actionName)}" title="${escapeHTML(actionName)}" ${busy ? 'disabled' : ''}>${content}</button>`;
  const cards = count ? Array.from({ length: layers }, (_, index) => {
    const top = index === layers - 1;
    if (top && action) return control(cardImage(face));
    return `<div class="bp-card${top && face ? '' : ' bp-card--back'}">${cardImage(top ? face : null, top ? face ? cardName(face) : 'Face-down cards' : '')}</div>`;
  }).join('') : action ? control('<span>Recycle<br>and draw</span>') : 'Empty';
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
  return `<section class="bp-table game-table" aria-label="Card table">${hand(1)}<div class="bp-piles">${pile('Draw pile', state.deck_count, null, turnActions().find(move => move.type === 'draw'))}${pile('Discards', state.pile_count, faceUp ? state.top : null, state.legal_moves.find(move => move.type === 'challenge'))}<div class="bp-declared"><p class="bp-declared__title">${faceUp ? 'Top' : 'Declared'} ${cardMark(state.top)}</p></div></div><div class="bp-effects"><span>${effectsText()}</span></div>${state.winner !== null ? `<p class="game-winner">${playerName(state.winner)} wins.</p>` : state.provisional_winner !== null ? `<p class="game-pending-win">${playerName(state.provisional_winner)} is out for now. The return attempt is still in play.</p>` : ''}<div class="game-player-result">${hand(0)}${moveExplain()}</div></section>`;
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
  if (state.phase === 'response') return 'Choosing a hand card or declaration accepts the last play.';
  if (!actual) return 'Choose an actual card from the active player’s hand.';
  if (!declared) return 'Choose the card identity to declare.';
  const matching = state.legal_moves.filter(move => move.type === 'play' && move.actual === actual && move.declared === declared);
  if (!matching.length) {
    if (state.skip_pending) return 'An ace is pending. Declare an ace to counter it, or skip.';
    if (state.draw_penalty) return state.top === 'KS' ? 'Counter this king with a declared 7 of spades.' : 'Use a legal seven, or king of spades on 7 of spades.';
    return `Match ${state.chosen_suit ? suits[state.chosen_suit] : `${suits[suit(state.top)]} or ${rankNames[rank(state.top)] || rank(state.top)}`}, or declare a queen.`;
  }
  if (rank(declared) === 'Q' && !chosenSuit) return 'Choose the continuing suit before playing the queen.';
  return finalPlay() ? '' : 'This combination is not available in the current turn.';
}

function declarationEffect() {
  if (!declared) return '';
  if (rank(declared) === '7') return 'Adds a 2-card penalty if accepted.';
  if (declared === 'KS') return 'Adds a 4-card penalty if accepted.';
  if (rank(declared) === 'A') return 'Passes a skip to the opponent if accepted.';
  if (rank(declared) === 'Q') return 'Sets the continuing suit if accepted.';
  return 'No special effect if accepted.';
}

function actionButton(move) {
  return `<button id="move-${move.type}" type="button" class="bp-button bp-button--${move.type === 'accept' ? 'primary' : 'secondary'} bp-button--wide" data-action="${move.type}" ${busy ? 'disabled' : ''} aria-label="${move.type === 'challenge' ? 'Call bluff on the declaration' : actionLabel(move.type)}">${move.type === 'accept' ? 'Accept' : move.type === 'challenge' ? 'Call bluff' : actionLabel(move.type).replace(/^Accept, then /, '')}</button>`;
}

function panel() {
  const move = finalPlay();
  const truthfulMove = finalPlay(actual);
  const needsSuit = (declared && rank(declared) === 'Q') || (actual && rank(actual) === 'Q' && state.legal_moves.some(item => item.type === 'play' && item.actual === actual && item.declared === actual));
  const reason = playReason();
  const responding = state.phase === 'response';
  const title = state.winner !== null ? `${playerName(state.winner)} wins` : `${playerName(state.turn)} ${responding ? 'to respond' : 'to act'}`;
  const responseMoves = state.legal_moves.filter(item => ['accept', 'challenge'].includes(item.type));
  return `<aside class="bp-action-panel game-panel" aria-labelledby="actor">
    <h2 id="actor" class="bp-action-panel__title" tabindex="-1">${title}</h2>
    <div class="game-selection">
      <div class="game-actual"><h3 class="bp-field__title">Actual card</h3>${actual && state.phase === 'turn' ? `<div class="bp-actual"><div class="bp-card bp-card--mini">${cardImage(actual, cardName(actual))}</div><p class="bp-actual__name">${cardName(actual)}</p></div>` : '<div class="bp-actual"><p class="game-actual-empty">Choose from your hand</p></div>'}</div>
      <div class="game-queen">${needsSuit ? `<h3 class="bp-field__title" id="suit-label">Continue with</h3><div class="game-suits" role="group" aria-labelledby="suit-label">${Object.entries(suits).map(([code, name]) => `<button id="suit-${code}" type="button" class="game-suit" data-suit="${code}" aria-pressed="${chosenSuit === code}" ${busy ? 'disabled' : ''}>${suitMark(code)}<span class="bp-sr-only">${name[0].toUpperCase() + name.slice(1)}</span></button>`).join('')}</div>` : ''}</div>
    </div>
    ${declarations()}
    <button id="play-truthful" type="button" class="bp-button bp-button--secondary bp-button--wide game-truthful" ${truthfulMove ? `data-move="${truthfulMove.id}"` : ''} ${!truthfulMove || busy ? 'disabled' : ''}>Play ${actual ? `${cardMark(actual)} ` : ''}as itself</button>
    <p class="bp-play-summary">${actual && declared && state.phase === 'turn' ? `Play ${cardMark(actual)} as ${cardMark(declared)}` : 'Choose your declaration'}</p>
    <p id="play-reason" class="game-play-reason" data-invalid="${state.phase === 'turn' && !!declared && !!reason}">${escapeHTML(reason || declarationEffect())}</p>
    <div class="bp-actions">${responseMoves.length ? `<div class="bp-actions game-response-actions">${responseMoves.map(actionButton).join('')}</div>` : `<button id="play-card" type="button" class="bp-button bp-button--primary bp-button--wide" ${move ? `data-move="${move.id}"` : ''} ${reason ? 'aria-describedby="play-reason"' : ''} ${!move || busy ? 'disabled' : ''}>Play face down</button>`}${turnActions().map(actionButton).join('')}</div>
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
  announcement.textContent = state.move_explain ? `${state.move_explain.title} ${state.move_explain.detail}` : state.winner !== null ? `${playerName(state.winner)} wins.` : `${playerName(state.turn)} ${state.phase === 'response' ? 'must accept or challenge' : 'to act'}. ${effectsText()}.`;
}

function select(intent) {
  if (intent.card && state.hands[state.turn].includes(intent.card)) {
    if (intent.card !== actual && (!declared || rank(declared) !== 'Q')) chosenSuit = null;
    actual = intent.card;
  }
  else if (intent.declared) {
    declared = intent.declared;
    chosenSuit = null;
  } else if (intent.suit) chosenSuit = intent.suit;
  else return false;
  announcement.textContent = !declared && finalPlay(actual) ? `${cardName(actual)} selected. Ready to play as itself.` : !declared && actual && rank(actual) === 'Q' && state.legal_moves.some(item => item.type === 'play' && item.actual === actual && item.declared === actual) ? 'Choose the continuing suit before playing the queen.' : playReason() || `${actual ? cardName(actual) : 'Card'} selected as ${declared ? cardName(declared) : 'no declaration'}. Ready to play.`;
  return true;
}

async function request(path, body, intent = null) {
  if (busy) return;
  const focused = document.activeElement.id;
  const actor = state?.turn;
  busy = true;
  const error = document.querySelector('#error');
  error.hidden = true;
  game.setAttribute('aria-busy', 'true');
  document.querySelector('#new-game').disabled = true;
  render();
  announcement.textContent = body ? 'Updating the table…' : 'Loading the table…';
  let success = false;
  let selected = false;
  async function update(path, body) {
    const response = await fetch(path, { method: body ? 'POST' : 'GET', headers: body ? { 'Content-Type': 'application/json' } : {}, body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(10000) });
    const next = await response.json();
    if (!response.ok) throw new Error(next.error || 'The table could not be updated.');
    receive(next);
    success = true;
  }
  try {
    await update(path, body);
    // Accept can resolve a return penalty, skip an empty hand, or finish the game.
    if (intent && state.phase === 'turn' && state.turn === actor && state.winner === null) {
      if (intent.action) {
        const move = state.legal_moves.find(item => item.type === intent.action);
        if (move) await update('/api/move', { version: state.version, move_id: move.id });
      } else selected = select(intent);
    }
  } catch (failure) {
    error.innerHTML = `<span>${escapeHTML(failure.name === 'TimeoutError' ? 'The table took too long to respond. Refresh its state before trying again.' : failure.message)}</span><button id="retry" type="button" class="bp-button bp-button--secondary bp-button--compact">Refresh table</button>`;
    error.hidden = false;
    if (!state) game.innerHTML = '<p class="game-loading">The table is unavailable. Use Refresh table to reconnect.</p>';
  } finally {
    busy = false;
    game.setAttribute('aria-busy', 'false');
    document.querySelector('#new-game').disabled = false;
    render(success && !selected && (!!body || focused === 'retry'), focused);
    if (!state) document.querySelector('#retry')?.focus();
  }
}

document.addEventListener('click', event => {
  const button = event.target.closest('button');
  if (!button || button.disabled || busy) return;
  if (button.id === 'new-game') return void request('/api/new', {});
  if (button.id === 'retry') return void request(state ? '/api/state' : '/api/new', state ? undefined : {});
  if (!state || state.winner !== null) return;
  if (button.dataset.move !== undefined) {
    const move = state.legal_moves.find(item => item.id === Number(button.dataset.move));
    if (move && (move.type !== 'play' || move === finalPlay(button.id === 'play-truthful' ? actual : declared))) return void request('/api/move', { version: state.version, move_id: move.id });
    return;
  }
  const intent = button.dataset;
  if (!intent.card && !intent.declared && !intent.suit && !intent.action) return;
  if (intent.card && !state.hands[state.turn].includes(intent.card)) return;
  const move = state.legal_moves.find(item => item.type === intent.action);
  if (move) return void request('/api/move', { version: state.version, move_id: move.id });
  if (state.phase === 'response') {
    if (intent.action && !turnActions().some(item => item.type === intent.action)) return;
    const accept = state.legal_moves.find(item => item.type === 'accept');
    if (accept) return void request('/api/move', { version: state.version, move_id: accept.id }, { ...intent });
  } else if (select(intent)) render();
});

request('/api/new', {});
