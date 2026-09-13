// Run with: node test_truthful_play.cjs. No browser or third-party dependencies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const handlers = {};
const nodes = new Map();
const context = vm.createContext({ assert, URLSearchParams, location: { search: '?debug=1' }, AbortSignal, handlers, document: {
  activeElement: { id: '' },
  querySelector(selector) {
    if (!nodes.has(selector)) nodes.set(selector, { addEventListener() {}, setAttribute() {} });
    return nodes.get(selector);
  },
  addEventListener(type, handler) { handlers[type] = handler; },
} });
vm.runInContext(fs.readFileSync('web/app.js', 'utf8').replace(/\n(?:request\('\/api\/new', \{\}\)|start\(\));\s*$/, ''), context);
vm.runInContext(`(async () => {
  const legal = [
    { id: 1, type: 'play', actual: '7H', declared: '7H', chosen_suit: null },
    { id: 2, type: 'play', actual: '7H', declared: 'QS', chosen_suit: 'C' },
    { id: 3, type: 'play', actual: 'KC', declared: '7H', chosen_suit: null },
    { id: 4, type: 'play', actual: 'QC', declared: 'QC', chosen_suit: 'D' },
  ];
  const turn = { version: 2, phase: 'turn', turn: 0, winner: null, hands: [['7H', 'KC', 'QC'], ['8S']],
    legal_moves: legal, history: [], top: '8H', draw_penalty: 0, skip_pending: false, chosen_suit: null };
  const shortcut = () => panel().match(/<button id="play-truthful"[^>]*>/)[0];
  receive(turn);
  assert.equal(actual, null);
  assert.match(shortcut(), /disabled/);
  assert.doesNotMatch(panel(), /id="move-accept"/, 'No acceptance control without a pending declaration');
  select({ card: '7H' });
  assert.equal(finalPlay(actual), legal[0]);
  assert.doesNotMatch(shortcut(), /disabled/);
  select({ declared: 'QS' });
  select({ suit: 'C' });
  assert.equal(finalPlay(), legal[1]);
  assert.equal(finalPlay(actual), legal[0], 'Grid must not turn the shortcut into a bluff');
  const realRequest = request;
  let sent;
  request = (...args) => { sent = args; };
  handlers.click({ target: { closest: () => ({ id: 'retry' }) } });
  assert.equal(sent[0], '/api/state', 'In-game retry can refresh the current game');
  state = null;
  handlers.click({ target: { closest: () => ({ id: 'retry' }) } });
  assert.deepEqual(sent, ['/api/new', {}], 'Failed page startup must retry a fresh game');
  state = turn;
  handlers.click({ target: { closest: () => ({ id: 'play-truthful', dataset: { move: '1' } }) } });
  assert.equal(sent[1].move_id, 1, 'Click sends the truthful move rather than the grid move');
  sent = null;
  handlers.click({ target: { closest: () => ({ id: 'play-truthful', dataset: { move: '2' } }) } });
  assert.equal(sent, null, 'Reject a stale or bluff move ID on the shortcut');
  request = realRequest;
  select({ card: 'KC' });
  select({ declared: '7H' });
  assert.equal(finalPlay(), legal[2]);
  assert.equal(finalPlay(actual), undefined);
  assert.match(shortcut(), /disabled/);
  receive(turn);
  select({ card: 'QC' });
  assert.match(panel(), /id="suit-label"/);
  select({ declared: '7H' });
  assert.match(panel(), /id="suit-label"/, 'A non-queen bluff selection cannot hide truthful queen suits');
  assert.match(shortcut(), /disabled/);
  select({ suit: 'H' });
  assert.match(shortcut(), /disabled/);
  select({ suit: 'D' });
  assert.equal(finalPlay(actual), legal[3]);
  assert.doesNotMatch(shortcut(), /disabled/);
  select({ card: '7H' });
  select({ card: 'QC' });
  assert.equal(chosenSuit, null, 'A newly selected truthful queen requires a fresh suit choice');
  assert.match(shortcut(), /disabled/);
  select({ suit: 'D' });
  busy = true;
  assert.match(shortcut(), /disabled/);
  busy = false;
  receive({ ...turn, legal_moves: legal.filter(item => item.id !== 4) });
  select({ card: 'QC' });
  assert.match(shortcut(), /disabled/);
  assert.doesNotMatch(panel(), /id="suit-label"/);
  assert.doesNotMatch(announcement.textContent, /continuing suit/, 'Do not announce hidden suit controls for an illegal truthful queen');
  const response = { ...turn, phase: 'response', legal_moves: [{ id: 0, type: 'accept' }, { id: 5, type: 'challenge' }, ...legal] };
  receive(response);
  render = () => {};
  request = (...args) => { sent = args; };
  sent = null;
  for (const dataset of [{ card: '7H' }, { declared: 'QS' }, { suit: 'C' }]) {
    handlers.click({ target: { closest: () => ({ dataset }) } });
    assert.equal(sent, null, 'Selection must never submit acceptance');
    assert.equal(state, response, 'Selection must leave the pending game unchanged');
  }
  assert.equal(finalPlay(), legal[1]);
  assert.doesNotMatch(shortcut(), /disabled/);
  assert.match(panel(), /id="move-accept"[^>]*data-move="0"/);
  assert.doesNotMatch(panel(), /id="move-(challenge|draw|skip)"/);
  assert.equal(select({ declared: '9D' }), false, 'Reject unavailable declaration intents');
  assert.equal(declared, 'QS');
  handlers.click({ target: { closest: () => ({ id: 'play-card', dataset: { move: '2' } }) } });
  assert.equal(sent[1].move_id, 2, 'Play submits the response move directly');
  assert.equal(drawPileAction(), undefined, 'Acceptance belongs to its explicit button, never the draw pile');
  sent = null;
  handlers.click({ target: { closest: () => ({ id: 'move-accept', dataset: { move: '0' } }) } });
  assert.deepEqual(sent, ['/api/move', { version: 2, move_id: 0 }], 'Accept submits exactly the engine acceptance move');
  busy = true;
  assert.match(panel().match(/<button id="move-accept"[^>]*>/)[0], /disabled/);
  busy = false;
  receive({ ...response, top: 'AH', skip_pending: true, legal_moves: [
    { id: 0, type: 'accept' }, { id: 5, type: 'challenge' },
    { id: 6, type: 'play', actual: 'QC', declared: 'AH', chosen_suit: null },
  ] });
  select({ card: 'QC' });
  assert.match(shortcut(), /disabled/, 'A queen cannot be played as itself under an ace');
  assert.doesNotMatch(panel(), /id="suit-label"/, 'An illegal truthful queen does not expose suit choices');
  assert.equal(select({ declared: 'QS' }), false, 'No queen declaration is allowed under an ace');
  select({ declared: 'AH' });
  assert.equal(finalPlay().id, 6, 'The actual queen can still bluff as an ace');
  assert.match(actionLabel('accept'), /skip this turn/);
  request = realRequest;
  let requests = 0;
  fetch = async () => { requests++; return { ok: true, json: async () => ({ ...turn, turn: 1 }) }; };
  await request('/api/move', { version: 2, move_id: 0 });
  assert.equal(requests, 1, 'Forced acceptance must not be followed by an extra draw');
  assert.equal(actual, null);
  receive({ ...turn, winner: 0, legal_moves: [] });
  assert.match(shortcut(), /disabled/);
})()`, context).then(() => console.log('Truthful-play checks passed.'), error => { console.error(error); process.exitCode = 1; });
