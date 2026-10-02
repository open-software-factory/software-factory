import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { win, animations } from './dom-env.ts';

const React = await import('react');
const { render, cleanup, act, screen, within } = await import('@testing-library/react');
const { default: userEvent } = await import('@testing-library/user-event');
const { CommandPalette, usePaletteShortcut } = await import('../src/palette.ts');
const { buildCommands } = await import('../../console-model/src/commands.ts');
const { fromPreset } = await import('../../console-model/src/layout.ts');
const { SURFACES, NAV_ORDER } = await import('../../console-model/src/surfaces.ts');
type Command = import('../../console-model/src/commands.ts').Command;
const h = React.createElement;

const state = fromPreset(SURFACES, {
  centre: { panels: ['floor', 'board', 'runway'], active: 'floor' },
  left: { panels: ['attention'], mode: 'rail' },
});
const commands = buildCommands({
  registry: SURFACES, state, order: NAV_ORDER, theme: 'dark', focused: false,
  workItems: [{ id: 'service-auth#5', title: 'Go-live: harden Keycloak availability', meta: 'P0' }],
  runs: [{ id: 'run-4471', title: 'run-4471 · SF-147 · attempt 3', surface: 'logs' }],
});

const ran: Command[] = [];
function Fixture() {
  const [open, setOpen] = React.useState(false);
  const triggerRef = React.useRef<HTMLButtonElement>(null);
  usePaletteShortcut(React.useCallback(() => setOpen(o => !o), []));
  return h('div', {},
    h('button', { ref: triggerRef, onClick: () => setOpen(true) }, 'Search'),
    h('button', {}, 'Outside action'),
    h(CommandPalette, { isOpen: open, onOpenChange: setOpen, commands, triggerRef, onRun: c => ran.push(c) }));
}

async function finishMotion() {
  await act(async () => {
    animations.filter(a => !a.cancelled).forEach(a => a.finish());
    await Promise.resolve();
  });
  await act(async () => { await new Promise(r => setTimeout(r, 35)); });
}
async function open() {
  render(h(Fixture));
  const user = userEvent.setup({ document: win.document });
  const trigger = screen.getByRole('button', { name: 'Search' });
  await user.click(trigger);
  await finishMotion();
  return { user, trigger, input: screen.getByRole('searchbox', { name: 'Search commands' }) as HTMLInputElement };
}
const rows = () => screen.getAllByRole('option').map(o => o.getAttribute('data-od-id'));
const focusedRow = () => document.querySelector('.od-palette-row[data-focused]')?.getAttribute('data-od-id') ?? null;

afterEach(async () => {
  cleanup(); animations.length = 0; ran.length = 0;
  await act(async () => { await new Promise(r => setTimeout(r, 25)); });
});

test('opens from the trigger with focus in the field and views listed first', async () => {
  const { input } = await open();
  assert.ok(document.activeElement === input, `Focus landed on ${document.activeElement?.tagName}`);
  assert.ok(screen.getByRole('dialog', { name: 'Command palette' }));
  assert.equal(rows()[0], 'view:floor');
  assert.equal(rows().some(id => id?.startsWith('move:')), false, 'move commands hidden until a query');
  assert.equal(screen.queryByRole('button', { name: 'Outside action' }), null, 'background hidden from AT');
});

test('Ctrl+P toggles it open and closed; the browser print shortcut is suppressed', async () => {
  render(h(Fixture));
  const user = userEvent.setup({ document: win.document });
  let prevented = false;
  // Same target, registered later: runs after the palette's capture listener.
  const spy = (e: KeyboardEvent) => { prevented = e.defaultPrevented; };
  window.addEventListener('keydown', spy, true);
  try {
    await user.keyboard('{Control>}p{/Control}');
    await finishMotion();
    assert.ok(screen.getByRole('dialog'));
    assert.ok(prevented, 'Ctrl+P must not reach the browser');
    await user.keyboard('{Control>}p{/Control}');
    await finishMotion();
    assert.equal(screen.queryByRole('dialog'), null);
  } finally { window.removeEventListener('keydown', spy, true); }
});

test('typing filters and ranks; Enter runs the top match and closes through the overlay', async () => {
  const { user, trigger } = await open();
  await user.keyboard('run');
  assert.equal(rows()[0], 'view:runway');
  assert.ok(rows().includes('run:run-4471'));
  assert.equal(focusedRow(), 'view:runway', 'first match takes the cursor as you type');
  await user.keyboard('{Enter}');
  assert.deepEqual(ran.map(c => c.id), ['view:runway']);
  assert.ok(document.querySelector('[data-phase="exiting"]'), 'closes through the shared lifecycle');
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
  assert.ok(document.activeElement === trigger, `Focus landed on ${document.activeElement?.tagName}`);
});

test('arrow keys move the cursor without leaving the field; Enter runs the chosen row', async () => {
  const { user, input } = await open();
  await user.keyboard('{ArrowDown}{ArrowDown}');
  assert.ok(document.activeElement === input, `Focus landed on ${document.activeElement?.tagName}`);
  assert.equal(focusedRow(), 'view:board');
  await user.keyboard('{Enter}');
  assert.deepEqual(ran.map(c => c.id), ['view:board']);
});

test('a query surfaces move commands with real placement intents', async () => {
  const { user } = await open();
  await user.keyboard('attention right');
  const row = screen.getByRole('option', { name: 'Move Attention to Right' });
  await user.click(row);
  assert.deepEqual(ran[0]?.intent, { kind: 'move', id: 'attention', to: 'right' });
});

test('work items are found by reference and by words in the title', async () => {
  const { user, input } = await open();
  await user.keyboard('#5');
  assert.deepEqual(rows(), ['item:service-auth#5']);
  await user.clear(input);
  await user.keyboard('keycloak');
  assert.deepEqual(rows(), ['item:service-auth#5']);
});

test('no match shows an honest empty state', async () => {
  const { user } = await open();
  await user.keyboard('zzzz');
  // React Aria announces the empty state as an option; no command rows remain.
  assert.equal(document.querySelectorAll('.od-palette-row').length, 0);
  assert.ok(within(screen.getByRole('dialog')).getByText('Nothing matches “zzzz”.'));
});

test('Escape closes in one step even with text in the field', async () => {
  const { user, trigger } = await open();
  await user.keyboard('boa');
  await user.keyboard('{Escape}');
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
  assert.ok(document.activeElement === trigger, `Focus landed on ${document.activeElement?.tagName}`);
  assert.equal(ran.length, 0);
});

test('clicking outside dismisses', async () => {
  const { user } = await open();
  await user.click(document.querySelector('[data-od-id="command-palette-backdrop"]')!);
  await finishMotion();
  assert.equal(screen.queryByRole('dialog'), null);
});

test('reopening starts with an empty query', async () => {
  const { user, trigger } = await open();
  await user.keyboard('boa');
  await user.keyboard('{Escape}');
  await finishMotion();
  await user.click(trigger);
  await finishMotion();
  assert.equal((screen.getByRole('searchbox') as HTMLInputElement).value, '');
  assert.equal(rows()[0], 'view:floor');
});
