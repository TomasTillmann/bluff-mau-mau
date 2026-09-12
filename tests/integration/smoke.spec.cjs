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
  const started = page.waitForResponse(response => response.url().endsWith('/api/new') && response.request().method() === 'POST');
  await page.goto('/');
  expect((await started).ok()).toBeTruthy();
  await ready(page);
  if (endpoint === '/api/debug/max-hand') {
    await page.locator('#hand-preset').selectOption(String(data.count));
    await ready(page);
  }
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
    const selectors = ['.game-seat', '.game-fan img', '.bp-declarations', '#play-truthful', '#play-card', '#move-draw', '.game-move-explain'];
    return {
      height: innerHeight, width: innerWidth, scrollY,
      elements: selectors.flatMap(selector => [...document.querySelectorAll(selector)].map((element, index) => {
        const rect = element.getBoundingClientRect();
        return { name: `${selector}[${index}]`, top: rect.top + scrollY, bottom: rect.bottom + scrollY, left: rect.left + scrollX, right: rect.right + scrollX };
      })),
    };
  });
  expect(geometry.scrollY, 'Core game controls must work without page scrolling').toBe(0);
  expect(geometry.elements.length, 'Both five-card hands, grid, three actions and result must be measured').toBe(17);
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
    const responseLayout = await page.evaluate(() => ({
      height: innerHeight, scrollHeight: document.documentElement.scrollHeight, scrollY,
      buttons: [...document.querySelectorAll('#move-accept, #move-challenge, #move-draw')].map(button => {
        const rect = button.getBoundingClientRect();
        return { id: button.id, top: rect.top + scrollY, bottom: rect.bottom + scrollY, width: rect.width, height: rect.height };
      }),
    }));
    expect(responseLayout.scrollHeight, 'Response history must not extend the page').toBeLessThanOrEqual(responseLayout.height);
    expect(responseLayout.scrollY, 'Responding must not require page scrolling').toBe(0);
    expect(responseLayout.buttons).toHaveLength(3);
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
    for (const declaration of ['7S', 'QC']) {
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
    await expect(page.locator('#hand-preset')).toHaveValue('');
  }
  await load(page);
  await pick(page, '10H');
  await page.locator('#declare-7C').click();
  await page.locator('#play-card').click();
  await ready(page);
  await pick(page, 'JC', 1);
  await page.locator('#declare-QH').click();
  await page.locator('#suit-S').click();
  const played = await (await page.request.get('/api/state')).json();
  expect(played.draw_penalty).toBe(2);
  expect(played.history.length).toBeGreaterThan(1);
  expect(played.move_explain).not.toBeNull();
  await expect(page.locator('#declare-QH')).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#suit-S')).toHaveAttribute('aria-pressed', 'true');
  await reloadFresh();
  await page.locator('#hand-preset').selectOption('31');
  await ready(page);
  await expect(page.locator('.game-winner')).toBeVisible();
  await reloadFresh();
});
