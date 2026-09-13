// Run with: node test_bot_picker.cjs. Pure search/render contracts; no browser or network.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

// Opening index.html from the desktop must reach the Rust server before its
// absolute asset paths resolve against the filesystem root.
const html = fs.readFileSync('web/index.html', 'utf8');
const inlineScripts = [...html.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)]
  .filter(match => !/\bsrc=/.test(match[0])).map(match => match[1]);
for (const [protocol, search, hash] of [['file:', '', ''], ['file:', '?debug=1', '#table'], ['http:', '?debug=1', '']]) {
  const redirects = [];
  const location = { protocol, search, hash, replace: url => redirects.push(url) };
  for (const script of inlineScripts) vm.runInNewContext(script, { location });
  assert.deepEqual(redirects, protocol === 'file:' ? [`http://127.0.0.1:8767/${search}${hash}`] : [],
    'File launch redirects to the server, preserves debug mode, and leaves HTTP alone');
}
const firstInline = html.search(/<script(?:\s[^>]*)?>/);
const firstAbsoluteAsset = html.search(/(?:href|src)="\//);
assert.ok(firstInline >= 0 && firstInline < firstAbsoluteAsset, 'File launch runs before absolute assets are requested');

const context = vm.createContext({ assert, URLSearchParams, location: { search: '' }, document: {
  querySelector: () => ({}),
  addEventListener() {},
} });
vm.runInContext(fs.readFileSync('web/app.js', 'utf8').replace(/\n(?:request\('\/api\/new', \{\}\)|start\(\));\s*$/, ''), context);
vm.runInContext(`
  const tactical = { id: 'Tactical[C0]', name: 'Tactical[C0]', family: 'Tactical' };
  const mixed = { id: 'MixedGreedy[B0-N100-C40]', name: 'MixedGreedy[B0-N100-C40]', family: 'MixedGreedy' };
  const different = { ...mixed, id: 'MixedGreedy[B10-N100-C40]', name: 'MixedGreedy[B10-N100-C40]' };
  assert.ok(fuzzyScore(tactical, 'tactcal') >= 0, 'A missing letter still finds Tactical');
  assert.ok(fuzzyScore(tactical, 'TACTICAL c0') >= 0, 'Search is case-insensitive and handles multiple tokens');
  assert.ok(fuzzyScore(mixed, 'b0 c40') >= 0, 'Parameter tokens select the requested configuration');
  assert.ok(fuzzyScore(different, 'b0 c40') < 0, 'B0 must not accidentally match B10');
  assert.ok(fuzzyScore(tactical, '') >= 0, 'Empty search includes bots');
  assert.ok(fuzzyScore(tactical, 'zzzxqv-no-such-engine') < 0, 'Unrelated text has no match');
  state = { mode: 'play', human_player: 0, bot: tactical, phase: 'turn', turn: 0, winner: null,
    hands: [['7H'], []], hand_counts: [1, 3], legal_moves: [], history: [] };
  const hidden = hand(1);
  assert.equal((hidden.match(/data-opponent-card/g) || []).length, 3);
  assert.doesNotMatch(hidden, /data-card=/, 'Opponent back wrappers never carry card identities');
  assert.match(hand(0), /data-card="7H"/, 'The human can still see their own cards');
`, context);
console.log('Bot-picker checks passed.');
