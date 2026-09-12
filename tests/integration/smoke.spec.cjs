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
  const response = await page.request.post(endpoint, { data });
  expect(response.ok()).toBeTruthy();
  await page.goto('/');
  await ready(page);
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

test.beforeEach(async ({ page }) => {
  errors.set(page, []);
  page.on('pageerror', error => errors.get(page).push(error.message));
});
test.afterEach(async ({ page }) => expect(errors.get(page)).toEqual([]));

test('draw pile lifts at its visible edge and really draws', async ({ page }) => {
  await load(page);
  await expect(page.locator('#pile-challenge')).toHaveCount(0);
  const point = await hoverPile(page, '#pile-draw', true);
  await page.mouse.click(point.x, point.y);
  await ready(page);
  await expect(page.getByLabel('Last move', { exact: true })).toContainText('You drew 1 card.');
  const state = await (await page.request.get('/api/state')).json();
  expect(state.hands[0]).toHaveLength(6);
  expect(state.turn).toBe(1);
});

test('both players can call a bluff by clicking a two- or three-layer discard', async ({ page }) => {
  await load(page);
  for (const [player, actual, declared] of [[0, '10H', '10C'], [1, 'JC', '10H']]) {
    await pick(page, actual, player);
    await page.locator(`#declare-${declared}`).click();
    await page.locator('#play-card').click();
    await ready(page);
    await expect(page.locator('#pile-challenge')).toBeEnabled();
    const point = await hoverPile(page, '#pile-challenge', player === 1);
    await page.mouse.click(point.x, point.y);
    await ready(page);
    await expect(page.locator('#pile-challenge')).toHaveCount(0);
    const state = await (await page.request.get('/api/state')).json();
    expect(state.move_explain.kind).toBe('bluff_caught');
    expect(state.move_explain.actor).toBe(1 - player);
    expect(state.move_explain.drawn[player]).toBe(2);
  }
});

for (const width of [1272, 320]) {
  test(`card, queen and declaration choices keep panel controls still at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await load(page);
    const positions = await panelPositions(page);
    for (const card of ['KD', '10H', 'QS']) {
      await pick(page, card);
      samePositions(positions, await panelPositions(page));
    }
    await expect(page.locator('#play-truthful')).toBeDisabled();
    await page.locator('#suit-D').click();
    await expect(page.locator('#play-truthful')).toBeEnabled();
    samePositions(positions, await panelPositions(page));
    for (const declaration of ['7S', 'QC']) {
      await page.locator(`#declare-${declaration}`).click();
      samePositions(positions, await panelPositions(page));
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
