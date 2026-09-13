const { test, expect } = require('@playwright/test');

const errors = new WeakMap();
async function pick(page, card, player = 0) {
  const button = page.locator(`#hand-${player}-${card}`);
  // Use the visible corner, away from the empty triangle in a rotated card’s bounding box.
  await button.click({ position: { x: 16, y: 24 } });
  await expect(button).toHaveAttribute('aria-pressed', 'true');
}
const ready = page => expect(page.locator('#game')).toHaveAttribute('aria-busy', 'false');

async function load(page, data = { seed: 0 }, endpoint = '/api/new') {
  await page.unroute('**/api/new');
  const seedData = endpoint === '/api/new' ? data : { seed: 0 };
  await page.route('**/api/new', route => route.continue({ postData: JSON.stringify(seedData) }));
  if (endpoint !== '/api/new') {
    // Redirect only this startup request; subsequent reloads still create a fresh game.
    await page.route('**/api/new', route => route.continue({
      url: new URL(endpoint, route.request().url()).href,
      postData: JSON.stringify(data),
    }), { times: 1 });
  }
  const started = page.waitForResponse(response => response.url().endsWith(endpoint) && response.request().method() === 'POST');
  await page.goto('/?debug=1');
  expect((await started).ok()).toBeTruthy();
  await ready(page);
  await expect(page.locator('#hand-preset')).toHaveCount(0);
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all([...document.images].map(image => image.decode()));
  });
}

async function hoverPile(page, id, edge = false) {
  const button = page.locator(id);
  await button.scrollIntoViewIfNeeded();
  await page.mouse.move(2, 2);
  const image = button.locator('img');
  await expect(image).toHaveCSS('transform', 'none');
  const box = await button.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + (edge ? box.height + 4 : box.height / 2));
  await expect.poll(() => image.evaluate(el => new DOMMatrix(getComputedStyle(el).transform).m42)).toBeLessThan(-4);
  expect(await button.boundingBox(), 'Hover must not move the hit target').toEqual(box);
  expect(await image.evaluate(el => getComputedStyle(el).transitionDuration.split(',').some(value => parseFloat(value) > 0))).toBeTruthy();
  return { x: box.x + box.width / 2, y: box.y + (edge ? box.height + 4 : box.height / 2) };
}

async function panelPositions(page) {
  return page.evaluate(() => {
    return ['.bp-declarations', '#play-truthful', '#play-card'].map(selector => document.querySelector(selector).getBoundingClientRect().top + window.scrollY);
  });
}

function samePositions(before, after) {
  before.forEach((value, index) => expect(Math.abs(after[index] - value), 'Selection moved a panel control').toBeLessThan(1));
}

async function coreFitsViewport(page) {
  const geometry = await page.evaluate(async () => {
    await Promise.all([...document.querySelectorAll('.game-fan')].flatMap(fan => fan.getAnimations({ subtree: true }).map(animation => animation.finished)));
    const selectors = ['.game-seat', '.game-fan img', '.bp-declarations', '#play-truthful', '#play-card', '.game-move-explain'];
    return {
      height: innerHeight, width: innerWidth, scrollY,
      elements: selectors.flatMap(selector => [...document.querySelectorAll(selector)].map((element, index) => {
        const rect = element.getBoundingClientRect();
        return { name: `${selector}[${index}]`, top: rect.top + scrollY, bottom: rect.bottom + scrollY, left: rect.left + scrollX, right: rect.right + scrollX };
      })),
    };
  });
  expect(geometry.scrollY, 'Core game controls must work without page scrolling').toBe(0);
  expect(geometry.elements.length, 'Both five-card hands, grid, two actions and result must be measured').toBe(16);
  for (const box of geometry.elements) {
    expect(box.top, `${box.name} starts above the viewport`).toBeGreaterThanOrEqual(-0.5);
    expect(box.bottom, `${box.name} ends below the viewport`).toBeLessThanOrEqual(geometry.height + 0.5);
    expect(box.left, `${box.name} starts outside the viewport`).toBeGreaterThanOrEqual(-0.5);
    expect(box.right, `${box.name} ends outside the viewport`).toBeLessThanOrEqual(geometry.width + 0.5);
  }
}

test.beforeEach(async ({ page }) => {
  errors.set(page, []);
  page.on('pageerror', error => errors.get(page).push(error.message));
});
test.afterEach(async ({ page }) => expect(errors.get(page)).toEqual([]));

test('draw pile lifts, draws and swaps the active hand', async ({ page }) => {
  async function checkHands(activePlayer) {
    for (const player of [0, 1]) {
      await expect(page.locator(`#player-${player}`)).toHaveText(`Player ${player + 1}`);
      const fan = page.locator(`[aria-labelledby="player-${player}"] .game-fan`);
      if (player === activePlayer) {
        await expect(fan).toHaveCSS('opacity', '1');
        await expect(fan.locator('[data-card]').last()).toBeEnabled();
      } else {
        await expect.poll(() => fan.evaluate(el => Number(getComputedStyle(el).opacity))).toBeLessThan(1);
        const card = fan.locator('[data-card]').last();
        await expect(card).toBeDisabled();
        await card.scrollIntoViewIfNeeded();
        await page.mouse.move(2, 2);
        const transforms = () => card.evaluate(el => [el, el.querySelector('img')].map(node => getComputedStyle(node).transform));
        const resting = await transforms();
        const box = await card.boundingBox();
        await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
        await card.evaluate(async el => {
          await Promise.all(el.getAnimations({ subtree: true }).map(animation => animation.finished));
        });
        expect(await transforms(), 'Inactive cards must not move on hover').toEqual(resting);
      }
    }
  }
  await load(page);
  await checkHands(0);
  await expect(page.locator('#pile-challenge')).toHaveCount(0);
  const point = await hoverPile(page, '#pile-draw', true);
  await page.mouse.click(point.x, point.y);
  await ready(page);
  await expect(page.getByLabel('Last move', { exact: true })).toContainText('You drew 1 card.');
  const state = await (await page.request.get('/api/state')).json();
  expect(state.hands[0]).toHaveLength(6);
  expect(state.turn).toBe(1);
  await checkHands(1);
});

test('both players can call a bluff by clicking a two- or three-layer discard', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await load(page);
  for (const [player, actual, declared] of [[0, '10H', 'QC'], [1, 'JC', '10H']]) {
    await pick(page, actual, player);
    await page.locator(`#declare-${declared}`).click();
    if (declared === 'QC') await page.locator('#suit-S').click();
    await page.locator('#play-card').click();
    await ready(page);
    await expect(page.locator('#pile-challenge')).toBeEnabled();
    if (declared === 'QC') await expect(page.locator('.game-declared-suit')).toHaveText('Now spades');
    else await expect(page.locator('.game-declared-suit')).toHaveCount(0);
    const responseLayout = await page.evaluate(() => ({
      height: innerHeight, scrollHeight: document.documentElement.scrollHeight, scrollY,
      buttons: [...document.querySelectorAll('#play-card, #play-truthful, #move-accept')].map(button => {
        const rect = button.getBoundingClientRect();
        return { id: button.id, top: rect.top + scrollY, bottom: rect.bottom + scrollY, width: rect.width, height: rect.height };
      }),
    }));
    expect(responseLayout.scrollHeight, 'Response history must not extend the page').toBeLessThanOrEqual(responseLayout.height);
    expect(responseLayout.scrollY, 'Responding must not require page scrolling').toBe(0);
    expect(responseLayout.buttons).toHaveLength(3);
    await expect(page.locator('#move-challenge, #move-draw, #move-skip')).toHaveCount(0);
    for (const box of responseLayout.buttons) {
      expect(box.top, `${box.id} starts above the viewport`).toBeGreaterThanOrEqual(0);
      expect(box.bottom, `${box.id} ends below the viewport`).toBeLessThanOrEqual(responseLayout.height);
      expect(box.width).toBeGreaterThan(0);
      expect(box.height).toBeGreaterThan(0);
    }
    const point = await hoverPile(page, '#pile-challenge', player === 1);
    await page.mouse.click(point.x, point.y);
    await ready(page);
    await expect(page.locator('#pile-challenge')).toHaveCount(0);
    await expect(page.locator('.game-declared-suit')).toHaveCount(0);
    const state = await (await page.request.get('/api/state')).json();
    expect(state.move_explain.kind).toBe('bluff_caught');
    expect(state.move_explain.actor).toBe(1 - player);
    expect(state.move_explain.drawn[player]).toBe(2);
  }
});

for (const [width, height] of [[1272, 812], [1280, 720], [320, 900]]) {
  test(`card, queen and declaration choices keep controls stable at ${width}x${height}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await load(page);
    const positions = await panelPositions(page);
    const checkLayout = async () => {
      samePositions(positions, await panelPositions(page));
      if (width > 880) await coreFitsViewport(page);
    };
    await checkLayout();
    const targets = await page.locator('.bp-declaration').evaluateAll(buttons => buttons.map(button => { const { width, height } = button.getBoundingClientRect(); return { width, height }; }));
    expect(targets).toHaveLength(32);
    for (const target of targets) {
      expect(target.width, 'Declaration target width').toBeGreaterThanOrEqual(44);
      expect(target.height, 'Declaration target height').toBeGreaterThanOrEqual(44);
    }
    for (const card of ['KD', '10H', 'QS']) {
      await pick(page, card);
      await checkLayout();
    }
    await expect(page.locator('#play-truthful')).toBeDisabled();
    await page.locator('#suit-D').click();
    await expect(page.locator('#play-truthful')).toBeEnabled();
    await checkLayout();
    await expect(page.locator('#declare-7S')).toBeDisabled();
    for (const declaration of ['7C', 'QC']) {
      await page.locator(`#declare-${declaration}`).click();
      await checkLayout();
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
  });
}

test('maximum hands stay in one fan and their end cards remain reachable', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 900 });
  for (const count of [30, 31]) {
    await load(page, { count }, '/api/debug/max-hand');
    const hand = page.locator('[aria-labelledby="player-0"]');
    await expect(hand.locator('.game-fan')).toHaveCount(1);
    const cards = hand.locator('[data-card]');
    await expect(cards).toHaveCount(count);
    await cards.last().scrollIntoViewIfNeeded();
    await expect(cards.last()).toBeInViewport();
    if (count === 30) {
      await cards.last().click();
      await expect(cards.last()).toHaveAttribute('aria-pressed', 'true');
      await cards.first().scrollIntoViewIfNeeded();
      await cards.first().click({ position: { x: 8, y: 24 } });
      await expect(cards.first()).toHaveAttribute('aria-pressed', 'true');
    } else await expect(cards.last()).toBeDisabled();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  }
});


test('reload clears play state, selections and finished debug presets', async ({ page }) => {
  async function reloadFresh() {
    const before = await (await page.request.get('/api/state')).json();
    await page.reload();
    await ready(page);
    const after = await (await page.request.get('/api/state')).json();
    expect(after.version).toBeGreaterThan(before.version);
    expect(after.turn).toBe(0);
    expect(after.phase).toBe('turn');
    expect(after.hands.map(hand => hand.length)).toEqual([5, 5]);
    expect([after.draw_penalty, after.skip_pending, after.winner, after.provisional_winner, after.chosen_suit]).toEqual([0, false, null, null, null]);
    expect(after.move_explain).toBeNull();
    expect(after.history).toHaveLength(1);
    await expect(page.locator('#actor')).toHaveText('Player 1 to act');
    await expect(page.locator('[data-card][aria-pressed="true"], [data-declared][aria-pressed="true"], [data-suit][aria-pressed="true"]')).toHaveCount(0);
    await expect(page.getByLabel('Last move', { exact: true })).toHaveText('');
    await expect(page.locator('.game-history .bp-history__row')).toHaveCount(1);
    await expect(page.locator('.game-history')).toContainText('New game.');
  }
  await load(page);
  await pick(page, '10H');
  await page.locator('#declare-8C').click();
  await page.locator('#play-card').click();
  await ready(page);
  await pick(page, 'JC', 1);
  await page.locator('#declare-QH').click();
  await page.locator('#suit-S').click();
  const played = await (await page.request.get('/api/state')).json();
  expect(played.draw_penalty).toBe(0);
  expect(played.history.length).toBeGreaterThan(1);
  expect(played.move_explain).not.toBeNull();
  await expect(page.locator('#declare-QH')).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#suit-S')).toHaveAttribute('aria-pressed', 'true');
  await reloadFresh();
  // A pending penalty and queen suit selection are separate legal states.
  await pick(page, '10H');
  await page.locator('#declare-7C').click();
  await page.locator('#play-card').click();
  await ready(page);
  await pick(page, 'JC', 1);
  await page.locator('#declare-7D').click();
  expect((await (await page.request.get('/api/state')).json()).draw_penalty).toBe(2);
  await expect(page.locator('#declare-7D')).toHaveAttribute('aria-pressed', 'true');
  await reloadFresh();
  await load(page, { count: 31 }, '/api/debug/max-hand');
  await expect(page.locator('.game-winner')).toBeVisible();
  await reloadFresh();
});


test('response selections stay local; ace counters, explicit acceptance and drawing resolve once', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await load(page);
  await pick(page, '10H');
  await page.locator('#declare-AC').click();
  await page.locator('#play-card').click();
  await ready(page);
  const pending = await (await page.request.get('/api/state')).json();
  const positions = await panelPositions(page);
  await pick(page, 'QH', 1);
  await expect(page.locator('#play-truthful')).toBeDisabled();
  await expect(page.locator('.game-suit')).toHaveCount(0);
  for (const suit of ['H', 'D', 'C', 'S']) await expect(page.locator(`#declare-Q${suit}`)).toBeDisabled();
  await pick(page, 'JC', 1);
  await page.locator('#declare-AS').click();
  samePositions(positions, await panelPositions(page));
  expect((await (await page.request.get('/api/state')).json()).version).toBe(pending.version);
  await expect(page.locator('#pile-challenge')).toBeEnabled();
  await expect(page.locator('#move-accept')).toBeEnabled();
  await expect(page.locator('#pile-draw, #pile-accept')).toHaveCount(0);
  await expect(page.locator('#declare-9H')).toBeDisabled();
  await page.locator('#play-card').click();
  await ready(page);
  const countered = await (await page.request.get('/api/state')).json();
  expect([countered.phase, countered.turn, countered.skip_pending, countered.top]).toEqual(['response', 0, true, 'AS']);
  expect(countered.version).toBe(pending.version + 1);
  expect(countered.move_explain.detail).toContain('Accepted ace of clubs');
  await expect(page.locator('#move-accept')).toHaveText('Accept declaration');
  await expect(page.locator('#move-accept')).toHaveAttribute('title', 'Accept the ace and skip this turn');
  await page.locator('#move-accept').click();
  await ready(page);
  const skipped = await (await page.request.get('/api/state')).json();
  expect([skipped.version, skipped.phase, skipped.turn, skipped.skip_pending]).toEqual([countered.version + 1, 'turn', 1, false]);
  expect(skipped.hands).toEqual(countered.hands);
  expect(skipped.move_explain.detail).toContain('skipped');
  await expect(page.locator('#move-accept')).toHaveCount(0);
  await pick(page, 'QH', 1);
  await expect(page.locator('#play-truthful')).toBeDisabled();
  await expect(page.locator('.game-suit')).toHaveCount(0);
  for (const suit of ['H', 'D', 'C', 'S']) await expect(page.locator(`#declare-Q${suit}`)).toBeDisabled();
  await page.locator('#declare-8S').click();
  await page.locator('#play-card').click();
  await ready(page);
  const beforeDraw = await (await page.request.get('/api/state')).json();
  await page.locator('#pile-draw').click();
  await ready(page);
  const drawn = await (await page.request.get('/api/state')).json();
  expect(drawn.version).toBe(beforeDraw.version + 1);
  expect(drawn.hands[0]).toHaveLength(beforeDraw.hands[0].length + 1);
  expect(drawn.move_explain.kind).toBe('draw');
  expect(drawn.move_explain.detail).toContain('Accepted 8 of spades');
  await expect(page.locator('#pile-challenge')).toHaveCount(0);

  await load(page);
  await pick(page, '10H');
  await page.locator('#declare-8C').click();
  await page.locator('#play-card').click();
  await ready(page);
  const ordinary = await (await page.request.get('/api/state')).json();
  await page.locator('#move-accept').click();
  await ready(page);
  const accepted = await (await page.request.get('/api/state')).json();
  expect([accepted.version, accepted.phase, accepted.turn]).toEqual([ordinary.version + 1, 'turn', 1]);
  expect(accepted.hands).toEqual(ordinary.hands);
  await expect(page.locator('#pile-challenge, #move-accept')).toHaveCount(0);
});

test('starting aces allow normal play and Accept resolves an empty-handed response', async ({ page }) => {
  await load(page, { seed: 13 });
  const opening = await (await page.request.get('/api/state')).json();
  expect([opening.top, opening.opening_card, opening.skip_pending]).toEqual(['AD', true, false]);
  await expect(page.locator('#move-accept, #pile-skip')).toHaveCount(0);
  await expect(page.locator('#declare-AH')).toBeEnabled();
  await expect(page.locator('#declare-8H')).toBeDisabled();
  await pick(page, '10D');
  await expect(page.locator('#play-truthful')).toBeEnabled();
  await expect(page.locator('#pile-draw')).toHaveAttribute('title', 'Draw 1 card and end turn');
  await page.locator('#pile-draw').click();
  await ready(page);
  const drawn = await (await page.request.get('/api/state')).json();
  expect([drawn.version, drawn.turn, drawn.opening_card, drawn.skip_pending]).toEqual([opening.version + 1, 1, true, false]);
  expect(drawn.hands[0]).toHaveLength(opening.hands[0].length + 1);
  await pick(page, 'KD', 1);
  await page.locator('#play-truthful').click();
  await ready(page);
  const played = await (await page.request.get('/api/state')).json();
  expect([played.top, played.opening_card, played.skip_pending, played.draw_penalty]).toEqual(['KD', false, false, 0]);

  await load(page, { count: 30 }, '/api/debug/max-hand');
  await pick(page, '7H');
  await page.locator('#declare-QS').click();
  await page.locator('#suit-S').click();
  await page.locator('#play-card').click();
  await ready(page);
  await pick(page, 'AS', 1);
  await page.locator('#declare-9S').click();
  await page.locator('#play-card').click();
  await ready(page);
  await pick(page, '8H');
  await page.locator('#declare-QS').click();
  await page.locator('#suit-S').click();
  await page.locator('#play-card').click();
  await ready(page);
  const empty = await (await page.request.get('/api/state')).json();
  expect(empty.hands[1]).toEqual([]);
  await expect(page.locator('#pile-challenge')).toBeEnabled();
  await expect(page.locator('#pile-accept')).toHaveCount(0);
  await expect(page.locator('#move-accept')).toHaveText('Accept declaration');
  await page.locator('#move-accept').click();
  await ready(page);
  const finished = await (await page.request.get('/api/state')).json();
  expect([finished.version, finished.winner]).toEqual([empty.version + 1, 1]);
  expect(finished.hands).toEqual(empty.hands);
});


test('opening penalties are optional, but a first seven stacks and a failed challenge adds two', async ({ page }) => {
  for (const [seed, top, actual] of [[30, '7S', '10D'], [98, 'KS', 'AC']]) {
    await load(page, { seed });
    const opening = await (await page.request.get('/api/state')).json();
    expect([opening.top, opening.opening_card, opening.draw_penalty]).toEqual([top, true, 0]);
    await expect(page.locator('#pile-draw')).toHaveAttribute('title', 'Draw 1 card and end turn');
    await pick(page, actual);
    await page.locator('#declare-8S').click();
    await page.locator('#play-card').click();
    await ready(page);
    const played = await (await page.request.get('/api/state')).json();
    expect([played.top, played.opening_card, played.draw_penalty]).toEqual(['8S', false, 0]);
  }
  for (const [action, count] of [['draw', 4], ['challenge', 6]]) {
    await load(page, { seed: 103 });
    const opening = await (await page.request.get('/api/state')).json();
    expect(opening.top).toBe('7H');
    await pick(page, '7S');
    await page.locator('#declare-7S').click();
    await expect(page.locator('#play-reason')).toHaveText('Starts a 4-card penalty if accepted.');
    await page.locator('#play-truthful').click();
    await ready(page);
    const pending = await (await page.request.get('/api/state')).json();
    expect([pending.draw_penalty, pending.opening_card]).toEqual([4, false]);
    await page.locator(`#pile-${action}`).click();
    await ready(page);
    const resolved = await (await page.request.get('/api/state')).json();
    expect(resolved.hands[1]).toHaveLength(pending.hands[1].length + count);
    expect(resolved.move_explain.drawn).toEqual([0, count]);
  }
});

function privatePlayState(state) {
  expect(state.mode).toBe('play');
  expect(state.human_player).toBe(0);
  expect(state.hands[1]).toEqual([]);
  expect(state.hand_counts[0]).toBe(state.hands[0].length);
  for (const hidden of ['deck', 'pile', 'rng_state']) expect(state).not.toHaveProperty(hidden);
  for (const move of state.legal_moves) {
    if (move.type === 'play') expect(state.hands[0]).toContain(move.actual);
  }
}

async function chooseNormalBot(page, query = 'Tactical') {
  await page.goto('/');
  await expect(page.locator('#bot-search')).toBeVisible();
  await ready(page);
  await page.locator('#bot-search').fill(query);
  const row = page.locator('button[data-bot-id]').first();
  await expect(row).toHaveAccessibleName(new RegExp(query));
  const id = await row.getAttribute('data-bot-id');
  const started = page.waitForResponse(response => response.url().endsWith('/api/play/new') && response.request().method() === 'POST');
  await row.click();
  const response = await started;
  expect(response.ok()).toBeTruthy();
  expect(response.request().postDataJSON().bot_id).toBe(id);
  const state = await response.json();
  privatePlayState(state);
  await ready(page);
  return state;
}

test('opponent loading recovers from a failed request through Retry', async ({ page }) => {
  let releaseCatalog;
  const pendingCatalog = new Promise(resolve => { releaseCatalog = resolve; });
  await page.route('**/api/bots', async route => {
    await pendingCatalog;
    await route.abort('failed');
  }, { times: 1 });
  await page.goto('/');
  try {
    await expect(page.locator('#game')).toHaveAttribute('aria-busy', 'true');
    await expect(page.locator('#bot-search')).toBeDisabled();
  } finally { releaseCatalog(); }
  await expect(page.locator('#retry')).toBeVisible();
  await ready(page);
  await expect(page.locator('#game')).not.toContainText('Loading opponents');
  await expect(page.locator('#error')).toBeVisible();

  const loaded = page.waitForResponse(response => response.url().endsWith('/api/bots'));
  await page.locator('#retry').click();
  expect((await loaded).ok()).toBeTruthy();
  await expect(page.locator('#bot-search')).toBeVisible();
  await expect(page.locator('#bot-search')).toBeEnabled();
  await expect(page.locator('#error')).not.toBeVisible();
  await page.locator('#bot-search').fill('Tactical');
  const started = page.waitForResponse(response => response.url().endsWith('/api/play/new') && response.request().method() === 'POST');
  await page.getByRole('button', { name: 'Play against Tactical[C0]', exact: true }).click();
  const response = await started;
  expect(response.ok()).toBeTruthy();
  const state = await response.json();
  privatePlayState(state);
  await ready(page);
  await expect(page.locator('#player-1 strong')).toHaveText(state.bot.name);
});

test('normal startup finds a bot from a typo and keeps opponent cards private', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#bot-search')).toBeVisible();
  await ready(page);
  const layout = await page.evaluate(async () => {
    await document.fonts.ready;
    const results = document.querySelector('.bot-list');
    return {
      height: innerHeight, pageHeight: document.documentElement.scrollHeight,
      width: innerWidth, pageWidth: document.documentElement.scrollWidth,
      listHeight: results.clientHeight, contentsHeight: results.scrollHeight,
    };
  });
  expect(layout.pageHeight, 'The desktop chooser scrolls its results, not the whole page').toBeLessThanOrEqual(layout.height + 1);
  expect(layout.pageWidth).toBeLessThanOrEqual(layout.width);
  expect(layout.contentsHeight).toBeGreaterThan(layout.listHeight);
  const catalog = await (await page.request.get('/api/bots')).json();
  const tactical = catalog.bots.find(bot => bot.id === 'Tactical[C0]');
  expect(tactical).toBeTruthy();
  await page.locator('#bot-search').fill('tactcal');
  const row = page.locator('button[data-bot-id="Tactical[C0]"]');
  await expect(row).toBeVisible();
  const metadata = page.locator('li.bot-row').filter({ has: row });
  await expect(metadata).toContainText(/Elo/i);
  await expect(metadata).toContainText(/W.*D.*L|wins|draws|losses/i);
  const numbers = (await metadata.innerText()).replace(/[,\s]/g, '');
  expect(numbers).toContain(String(tactical.wins));
  await page.locator('#bot-search').fill('zzzxqv-no-such-engine');
  await expect(page.locator('button[data-bot-id]')).toHaveCount(0);
  await page.locator('#bot-search').fill('tactcal');
  const started = page.waitForResponse(response => response.url().endsWith('/api/play/new') && response.request().method() === 'POST');
  await row.click();
  const response = await started;
  expect(response.ok()).toBeTruthy();
  const state = await response.json();
  privatePlayState(state);
  await ready(page);
  await expect(page.locator('#player-0')).toHaveText('You');
  await expect(page.locator('#player-1 strong')).toHaveText(tactical.name);
  await expect(page.locator('#debug-status')).not.toBeVisible();
  const backs = page.locator('[data-opponent-card]');
  await expect(backs).toHaveCount(state.hand_counts[1]);
  await expect(backs.locator('[data-card]')).toHaveCount(0);
  for (const image of await backs.locator('img').all()) {
    await expect(image).not.toHaveAttribute('src', /(?:^|\/)(?:[2-9]|10|[JQKA])[HDCS]\.[a-z]+$/i);
  }
});

test('a human move lets the selected bot act and returns a private human turn', async ({ page }) => {
  const before = await chooseNormalBot(page);
  expect(before.turn).toBe(0);
  expect(before.hand_counts).toEqual([5, 5]);
  await expect(page.locator('#pile-draw')).toBeEnabled();
  const moved = page.waitForResponse(response => response.url().endsWith('/api/play/move') && response.request().method() === 'POST');
  await page.locator('#pile-draw').click();
  const response = await moved;
  expect(response.ok()).toBeTruthy();
  const after = await response.json();
  privatePlayState(after);
  expect(after.version).toBeGreaterThan(before.version);
  expect(after.turn).toBe(after.human_player);
  expect(after.hand_counts[0]).toBe(6);
  expect(after.hand_counts[1]).toBeLessThan(before.hand_counts[1]);
  expect(after.history.length).toBeGreaterThan(before.history.length + 1);
  await ready(page);
  await expect(page.locator('#player-0')).toHaveText('You');
  await expect(page.locator('[data-opponent-card]')).toHaveCount(after.hand_counts[1]);
});

test('normal rematch retains the bot and the chooser can select another on mobile', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const first = await chooseNormalBot(page);
  const rematch = page.waitForResponse(response => response.url().endsWith('/api/play/new') && response.request().method() === 'POST');
  await page.locator('#new-game').click();
  const response = await rematch;
  expect(response.ok()).toBeTruthy();
  expect(response.request().postDataJSON().bot_id).toBe(first.bot.id);
  const again = await response.json();
  expect(again.bot.id).toBe(first.bot.id);
  expect(again.hand_counts).toEqual([5, 5]);
  privatePlayState(again);
  await ready(page);
  await page.locator('#choose-bot').click();
  await expect(page.locator('#bot-search')).toBeVisible();
  await expect(page.locator('#player-0')).not.toBeVisible();
  await page.locator('#bot-search').fill('Honest');
  const selected = page.waitForResponse(response => response.url().endsWith('/api/play/new') && response.request().method() === 'POST');
  await page.getByRole('button', { name: 'Play against HonestFirst[B0-N0-C0]', exact: true }).click();
  const next = await (await selected).json();
  expect(next.bot.id).toBe('HonestFirst[B0-N0-C0]');
  privatePlayState(next);
  await ready(page);
  await expect(page.locator('#player-1 strong')).toHaveText(next.bot.name);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});
