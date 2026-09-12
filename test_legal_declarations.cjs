// Run with: node test_legal_declarations.cjs. No browser or third-party dependencies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const context = vm.createContext({ assert, document: {
  querySelector: () => ({}),
  addEventListener() {},
} });
vm.runInContext(fs.readFileSync('web/app.js', 'utf8').replace(/\nrequest\('\/api\/new', \{\}\);\s*$/, ''), context);
vm.runInContext(`
  const legal = [
    { type: 'play', actual: '8D', declared: '9H', chosen_suit: null },
    { type: 'play', actual: '8D', declared: 'QS', chosen_suit: 'C' },
    { type: 'draw' },
  ];
  state = { phase: 'turn', turn: 0, winner: null, legal_moves: legal, hands: [['8D'], ['AC']] };
  const tile = card => declarations().match(new RegExp('<button id="declare-' + card + '"[^>]*>'))[0];
  assert.equal((declarations().match(/data-declared=/g) || []).length, 32);
  assert.equal(canDeclare('9H'), true, 'A legal declaration stays available even when the actual card differs');
  assert.equal(canDeclare('8D'), false, 'The actual card is not automatically a legal declaration');
  assert.equal(canDeclare('QS'), true, 'Queens remain selectable before a continuing suit is chosen');
  assert.equal(canDeclare('bad-card'), false);
  assert.doesNotMatch(tile('9H'), /disabled/);
  assert.doesNotMatch(tile('QS'), /disabled/);
  assert.match(tile('8D'), /disabled/);
  assert.doesNotMatch(hand(0), /disabled/, 'Actual hand cards remain selectable for bluffing');
  actual = 'AC';
  chosenSuit = 'H';
  assert.doesNotMatch(tile('9H'), /disabled/, 'Grid legality is independent of the selected actual card');
  assert.doesNotMatch(tile('QS'), /disabled/, 'Queen legality is independent of the chosen continuing suit');
  state.phase = 'response';
  state.legal_moves = [...legal, { type: 'challenge' }];
  assert.doesNotMatch(tile('9H'), /disabled/, 'Response phase uses the engine-provided play moves');
  assert.match(tile('8D'), /disabled/);
  busy = true;
  assert.equal((declarations().match(/ disabled/g) || []).length, 32);
  busy = false;
  state.winner = 0;
  assert.equal((declarations().match(/ disabled/g) || []).length, 32);
  state.winner = null;
  state.legal_moves = [{ type: 'challenge' }];
  assert.equal((declarations().match(/ disabled/g) || []).length, 32);
`, context);
console.log('Legal-declaration checks passed.');
