// Run with: node test_truthful_play.cjs. No browser or third-party dependencies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const handlers = {};
const nodes = new Map();
const context = vm.createContext({ assert, AbortSignal, handlers, document: {
  activeElement: { id: '' },
  querySelector(selector) {
    if (!nodes.has(selector)) nodes.set(selector, { addEventListener() {}, setAttribute() {} });
    return nodes.get(selector);
  },
  addEventListener(type, handler) { handlers[type] = handler; },
} });
vm.runInContext(fs.readFileSync('web/app.js', 'utf8').replace(/\nrequest\('\/api\/new', \{\}\);\s*$/, ''), context);
vm.runInContext(`(async () => {
  const legal = [
    { id: 1, type: 'play', actual: '7H', declared: '7H', chosen_suit: null },
    { id: 2, type: 'play', actual: '7H', declared: 'QS', chosen_suit: 'C' },
    { id: 3, type: 'play', actual: 'KC', declared: '7H', chosen_suit: null },
    { id: 4, type: 'play', actual: 'QC', declared: 'QC', chosen_suit: 'D' },
  ];
  const turn = { version: 2, phase: 'turn', turn: 0, winner: null, hands: [['7H', 'KC', 'QC'], ['8S']],
    legal_moves: legal, history: [], top: '7S', draw_penalty: 2, skip_pending: false, chosen_suit: null };
  const shortcut = () => panel().match(/<button id="play-truthful"[^>]*>/)[0];
  receive(turn);
  assert.equal(actual, null);
  assert.match(shortcut(), /disabled/);
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
  receive({ ...turn, phase: 'response', legal_moves: [{ id: 0, type: 'accept' }] });
  assert.match(shortcut(), /disabled/);
  render = () => {};
  fetch = async () => ({ ok: true, json: async () => turn });
  await request('/api/move', { version: 2, move_id: 0 }, { card: '7H' });
  assert.equal(actual, '7H', 'Response Accept keeps intended selection for same actor');
  receive({ ...turn, phase: 'response', legal_moves: [{ id: 0, type: 'accept' }] });
  fetch = async () => ({ ok: true, json: async () => ({ ...turn, turn: 1 }) });
  await request('/api/move', { version: 2, move_id: 0 }, { card: '7H' });
  assert.equal(actual, null, 'Response Accept must discard intent if actor changes');
  receive({ ...turn, winner: 0, legal_moves: [] });
  assert.match(shortcut(), /disabled/);
})()`, context).then(() => console.log('Truthful-play checks passed.'), error => { console.error(error); process.exitCode = 1; });
