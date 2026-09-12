async page => {
  const assert = (condition, message) => { if (!condition) throw new Error(message); };
  const errors = [];
  const failedRequests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('response', response => { if (response.status() >= 400) failedRequests.push(response.url()); });
  await page.goto('http://127.0.0.1:8766/design-system/');
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all([...document.images].map(image => image.decode()));
  });
  const components = ['.bp-topbar', '.bp-brand', '.bp-brand__mark', '.bp-debug', '.bp-hand-label', '.bp-card--mini', '.bp-card--back', '.bp-table', '.bp-pile', '.bp-pile__count', '.bp-declared', '.bp-status', '.bp-effects', '.bp-action-panel', '.bp-actual', '.bp-declarations', '.bp-play-summary', '.bp-penalty', '.bp-actions', '.bp-history', '.bp-footer'];
  for (const selector of components) assert(await page.locator(selector).count() > 0, 'Missing component: ' + selector);
  assert(await page.locator('.bp-declared .bp-suit[aria-hidden], .bp-play-summary .bp-suit[aria-hidden], .bp-history .bp-suit[aria-hidden]').count() === 0, 'Informative suit symbols must be exposed to assistive technology');
  const declarations = page.locator('.bp-declaration');
  assert(await declarations.count() === 32, 'Expected all 32 declarations');
  const names = await declarations.evaluateAll(items => items.map(item => item.getAttribute('aria-label')));
  assert(new Set(names).size === 32 && names.every(Boolean), 'Declaration names must be unique and accessible');
  for (const declaration of await declarations.all()) {
    assert(await declaration.isEnabled(), 'Every specimen declaration must be enabled');
    await declaration.click();
    assert(await declaration.getAttribute('aria-pressed') === 'true', 'Clicked declaration must be selected');
    assert(await page.locator('.bp-declaration[aria-pressed="true"]').count() === 1, 'Declaration selection must be exclusive');
  }
  const hands = page.locator('.bp-hand');
  assert(await hands.count() === 2, 'Both fixed-seat hands must be present');
  for (const hand of await hands.all()) {
    const cards = hand.locator('button.bp-card');
    assert(await cards.count() === 5, 'Each specimen hand must have five cards');
    for (const card of await cards.all()) {
      await card.focus();
      await card.press('Space');
      assert(await card.getAttribute('aria-pressed') === 'true', 'Keyboard must select hand cards');
      assert(await hand.locator('[aria-pressed="true"]').count() === 1, 'Hand selection must be exclusive');
    }
  }
  await page.getByRole('button', { name: 'New game', exact: true }).click();
  assert(await page.locator('.bp-declaration[aria-pressed="true"]').getAttribute('aria-label') === 'Declare 7 of hearts', 'Reset must restore declaration');
  const layouts = [];
  for (const width of [1440, 1000, 390]) {
    await page.setViewportSize({ width, height: 1000 });
    await page.evaluate(() => window.scrollTo(0, 0));
    const layout = await page.evaluate(() => ({
      viewport: innerWidth,
      pageWidth: document.documentElement.scrollWidth,
      minimumTarget: Math.min(...[...document.querySelectorAll('.bp-declaration')].map(button => button.getBoundingClientRect().width)),
      fontLoaded: document.fonts.check('500 32px "Bricolage Grotesque"'),
      brokenImages: [...document.images].filter(image => !image.complete || image.naturalWidth === 0).length
    }));
    assert(layout.pageWidth <= width + 1, 'Page must not overflow horizontally at ' + width);
    assert(layout.minimumTarget >= 44, 'Declaration hit targets must remain at least 44px');
    assert(layout.fontLoaded && layout.brokenImages === 0, 'Font and supplied card images must load');
    layouts.push(layout);
  }
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const duration = await page.locator('.bp-hand button').first().evaluate(element => getComputedStyle(element).transitionDuration);
  assert(duration === '0s', 'Reduced motion must remove card transitions');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.evaluate(() => window.scrollTo(0, 0));
  assert(errors.length === 0, 'Browser errors: ' + errors.join('; '));
  assert(failedRequests.length === 0, 'Failed requests: ' + failedRequests.join('; '));
  return { checked: '21 component selectors, 32 clickable declarations, both keyboard-selectable hands, reset, responsive layout, assets and reduced motion', layouts, errors, failedRequests };
}
