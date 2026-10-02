import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { win, animations } from './dom-env.ts';

const React = await import('react');
const { render, cleanup, act, screen } = await import('@testing-library/react');
const { default: userEvent } = await import('@testing-library/user-event');
const { Overlay } = await import('../src/overlay.ts');
const h = React.createElement;

function Fixture({ dismissable = true, keyboardDisabled = false } = {}) {
  const [open, setOpen] = React.useState(false);
  return h('div', {},
    h('button', { onClick: () => setOpen(true) }, 'Open navigation'),
    h('button', {}, 'Outside action'),
    h(Overlay, {
      isOpen: open, onOpenChange: setOpen, label: 'Navigation', id: 'test-navigation',
      placement: 'left', isDismissable: dismissable, isKeyboardDismissDisabled: keyboardDisabled,
    },
    h('button', { onClick: () => setOpen(false) }, 'Close navigation'),
    h('button', { onClick: () => setOpen(false) }, 'Floor'),
    h('button', { onClick: () => setOpen(false) }, 'Board')),
  );
}

async function finishMotion() {
  await act(async () => {
    animations.filter(a => !a.cancelled).forEach(a => a.finish());
    await Promise.resolve();
  });
  // FocusScope restores focus on the frame after React commits the unmount.
  await act(async () => { await new Promise(r => setTimeout(r, 35)); });
}
async function open(options = {}) {
  render(h(Fixture, options));
  const user = userEvent.setup({ document: win.document });
  const trigger = screen.getByRole('button', { name: 'Open navigation' });
  await user.click(trigger);
  return { user, trigger };
}
afterEach(async () => {
  cleanup(); animations.length = 0;
  await act(async () => { await new Promise(r => setTimeout(r, 25)); });
});

test('closed overlay exposes no hidden navigation controls', () => {
  render(h(Fixture));
  assert.equal(screen.queryByRole('dialog'), null);
  assert.equal(screen.queryByRole('button', { name: 'Board', hidden: true }), null);
});

test('opening captures focus and hides background from accessibility tree', async () => {
  await open();
  const dialog = screen.getByRole('dialog', { name: 'Navigation' });
  assert.ok(dialog.contains(document.activeElement));
  assert.equal(screen.queryByRole('button', { name: 'Outside action' }), null);
  assert.equal(document.querySelector('[data-od-id="test-navigation-panel"]')?.getAttribute('data-phase'), 'entering');
  await finishMotion();
  assert.equal(document.querySelector('[data-od-id="test-navigation-panel"]')?.getAttribute('data-phase'), 'open');
});

test('Tab and Shift+Tab remain inside the open dialog', async () => {
  const { user } = await open();
  await finishMotion();
  const dialog = screen.getByRole('dialog');
  for (let i = 0; i < 5; i++) {
    await user.tab(); assert.ok(dialog.contains(document.activeElement));
  }
  for (let i = 0; i < 5; i++) {
    await user.tab({ shift: true }); assert.ok(dialog.contains(document.activeElement));
  }
});

test('Escape waits for exit motion, then restores trigger focus', async () => {
  const { user, trigger } = await open();
  await finishMotion();
  await user.keyboard('{Escape}');
  assert.ok(screen.getByRole('dialog'));
  assert.equal(document.querySelector('[data-phase="exiting"]') !== null, true);
  assert.equal(screen.queryByRole('button', { name: 'Outside action' }), null);
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
  assert.ok(document.activeElement === trigger, `Focus landed on ${document.activeElement?.tagName}`);
});

test('outside pointer interaction dismisses and restores focus', async () => {
  const { user, trigger } = await open();
  await finishMotion();
  await user.click(document.querySelector('[data-od-id="test-navigation-backdrop"]'));
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
  assert.ok(document.activeElement === trigger, `Focus landed on ${document.activeElement?.tagName}`);
});

test('touch outside also dismisses', async () => {
  const { user } = await open();
  await finishMotion();
  await user.pointer([{ keys: '[TouchA>]', target: document.querySelector('[data-od-id="test-navigation-backdrop"]') }, { keys: '[/TouchA]' }]);
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
});

test('choosing a destination closes through the same lifecycle', async () => {
  const { user, trigger } = await open();
  await finishMotion();
  await user.click(screen.getByRole('button', { name: 'Board' }));
  assert.ok(document.querySelector('[data-phase="exiting"]'));
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
  assert.ok(document.activeElement === trigger, `Focus landed on ${document.activeElement?.tagName}`);
});

test('a decision dialog can disable outside and keyboard dismissal', async () => {
  const { user } = await open({ dismissable: false, keyboardDisabled: true });
  await finishMotion();
  await user.keyboard('{Escape}');
  await user.click(document.querySelector('[data-od-id="test-navigation-backdrop"]'));
  await finishMotion();
  assert.ok(screen.getByRole('dialog'));
  await user.click(screen.getByRole('button', { name: 'Close navigation' }));
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
});

test('close during entry cancels prior animations and completes exit', async () => {
  const { user } = await open();
  await user.keyboard('{Escape}');
  assert.ok(animations.some(a => a.cancelled));
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
});

test('unmount during animation releases modal effects', async () => {
  await open();
  cleanup();
  await finishMotion();
  assert.equal(document.querySelector('[role="dialog"]'), null);
  assert.equal(document.body.style.overflow, '');
});

test('Escape closes the overlay before a window-level context shortcut', async () => {
  let outerEscapes = 0;
  const handler = (event: KeyboardEvent) => { if (event.key === 'Escape') outerEscapes++; };
  window.addEventListener('keydown', handler);
  try {
    const { user } = await open();
    await finishMotion();
    await user.keyboard('{Escape}');
    await finishMotion();
    assert.equal(outerEscapes, 0);
  } finally { window.removeEventListener('keydown', handler); }
});

test('reopening during exit keeps the new overlay mounted', async () => {
  const content = h('button', {}, 'Inside');
  const props = { label: 'Navigation', id: 'reversal', children: content, onOpenChange() {} };
  const view = render(h(Overlay, { ...props, isOpen: true }));
  await finishMotion();
  view.rerender(h(Overlay, { ...props, isOpen: false }));
  assert.ok(document.querySelector('[data-phase="exiting"]'));
  view.rerender(h(Overlay, { ...props, isOpen: true }));
  await finishMotion();
  assert.ok(screen.getByRole('dialog'));
  assert.ok(document.querySelector('[data-phase="open"]'));
});

function FloatingFixture() {
  const [open, setOpen] = React.useState(false);
  const triggerRef = React.useRef<HTMLButtonElement>(null);
  return h('div', {},
    h('button', { ref: triggerRef, onClick: () => setOpen(true) }, 'Select item'),
    h('button', {}, 'Outside action'),
    h(Overlay, { isOpen: open, onOpenChange: setOpen, modal: false, placement: 'right',
      label: 'Inspector', id: 'float', triggerRef },
    h('button', {}, 'Pin'),
    h('button', { onClick: () => setOpen(false) }, 'Close inspector')));
}
async function openFloating() {
  render(h(FloatingFixture));
  const user = userEvent.setup({ document: win.document });
  const trigger = screen.getByRole('button', { name: 'Select item' });
  await user.click(trigger);
  await finishMotion();
  return { user, trigger };
}

test('non-modal: content stays reachable and focus is not captured', async () => {
  const { user, trigger } = await openFloating();
  assert.ok(screen.getByRole('dialog', { name: 'Inspector' }));
  // Compare with ===: a failed assert.equal on DOM nodes stalls on serialisation.
  assert.ok(document.activeElement === trigger, 'selecting content must not steal focus');
  assert.ok(screen.getByRole('button', { name: 'Outside action' }), 'background not hidden from AT');
  assert.equal(document.querySelector('.od-overlay-backdrop'), null, 'no scrim');
  let clicked = 0;
  screen.getByRole('button', { name: 'Outside action' }).addEventListener('click', () => clicked++);
  await user.click(screen.getByRole('button', { name: 'Outside action' }));
  assert.equal(clicked, 1, 'outside content still receives clicks');
  await finishMotion();
  assert.ok(screen.getByRole('dialog'), 'outside interaction does not dismiss');
});

test('non-modal: Escape closes only from inside, then hands focus back', async () => {
  const { user, trigger } = await openFloating();
  trigger.focus();
  await user.keyboard('{Escape}');
  await finishMotion();
  assert.ok(screen.getByRole('dialog'), 'Escape outside the panel is not ours');
  screen.getByRole('button', { name: 'Pin' }).focus();
  await user.keyboard('{Escape}');
  assert.ok(document.querySelector('[data-phase="exiting"]'), 'exits through the shared lifecycle');
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
  assert.ok(document.activeElement === trigger, `Focus landed on ${document.activeElement?.tagName}`);
});

test('non-modal: closing from its own control unmounts after motion', async () => {
  const { user } = await openFloating();
  await user.click(screen.getByRole('button', { name: 'Close inspector' }));
  assert.ok(screen.getByRole('dialog'));
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
});

test('removing the trigger during a breakpoint change uses the supplied focus fallback', async () => {
  let changeViewport!: () => void;
  function ResponsiveFixture() {
    const [open, setOpen] = React.useState(false);
    const [narrow, setNarrow] = React.useState(true);
    const triggerRef = React.useRef<HTMLButtonElement>(null);
    const fallbackFocusRef = React.useRef<HTMLButtonElement>(null);
    changeViewport = () => { setNarrow(false); setOpen(false); };
    return h('div', {},
      narrow && h('button', { ref: triggerRef, onClick: () => setOpen(true) }, 'Open navigation'),
      h('button', { ref: fallbackFocusRef }, 'Focus mode'),
      h(Overlay, { isOpen: open, onOpenChange: setOpen, triggerRef, fallbackFocusRef,
        label: 'Navigation', id: 'responsive' }, h('button', {}, 'Inside')));
  }
  render(h(ResponsiveFixture));
  const user = userEvent.setup({ document: win.document });
  await user.click(screen.getByRole('button', { name: 'Open navigation' }));
  await finishMotion();
  act(() => changeViewport());
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
  assert.ok(document.activeElement === screen.getByRole('button', { name: 'Focus mode' }));
});
