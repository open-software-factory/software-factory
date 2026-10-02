import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { win } from './dom-env.ts';

const React = await import('react');
const { render, cleanup, act, screen } = await import('@testing-library/react');
const { default: userEvent } = await import('@testing-library/user-event');
const { Popover, ChoiceMenu } = await import('../src/popover.ts');
const h = React.createElement;

// jsdom has no layout; React Aria still positions, so give it a rect to work with.
win.HTMLElement.prototype.getBoundingClientRect = function () {
  return { x: 10, y: 10, top: 10, left: 10, right: 110, bottom: 40, width: 100, height: 30, toJSON() {} } as DOMRect;
};

const picked: string[] = [];
function Fixture() {
  const [env, setEnv] = React.useState('prod');
  return h('div', {},
    h('button', {}, 'Outside action'),
    h(ChoiceMenu, {
      id: 'env', label: 'Environment scope', trigger: `${env}`, selected: env,
      onSelect: (id: string) => { picked.push(id); setEnv(id); },
      choices: [
        { id: 'prod', label: 'prod', hint: 'healthy' },
        { id: 'staging', label: 'staging', hint: 'degraded' },
        { id: 'dev', label: 'dev', hint: 'idle' },
      ],
    }),
    h(Popover, { id: 'attention', label: 'Needs you', trigger: '3 need you' },
      h('button', {}, 'First item')));
}

async function settle() {
  await act(async () => { await new Promise(r => setTimeout(r, 40)); });
}
function setup() {
  render(h(Fixture));
  return userEvent.setup({ document: win.document });
}
afterEach(async () => { cleanup(); picked.length = 0; await settle(); });

test('a popover opens beside its trigger as a labelled dialog and takes focus', async () => {
  const user = setup();
  await user.click(screen.getByRole('button', { name: '3 need you' }));
  await settle();
  const dialog = screen.getByRole('dialog', { name: 'Needs you' });
  assert.ok(dialog.contains(document.activeElement), 'focus moved into the popover');
  assert.ok(document.querySelector('[data-od-id="attention-popover"]'));
});

test('Escape closes the popover and returns focus to the trigger', async () => {
  const user = setup();
  const trigger = screen.getByRole('button', { name: '3 need you' });
  await user.click(trigger);
  await settle();
  await user.keyboard('{Escape}');
  await settle();
  assert.equal(screen.queryByRole('dialog', { name: 'Needs you' }), null);
  assert.ok(document.activeElement === trigger, `Focus landed on ${document.activeElement?.tagName}`);
});

test('clicking outside closes the popover', async () => {
  const user = setup();
  await user.click(screen.getByRole('button', { name: '3 need you' }));
  await settle();
  // The popover hides the rest of the page from assistive tech while open.
  const outside = screen.getByRole('button', { name: 'Outside action', hidden: true });
  await user.click(outside);
  await settle();
  assert.equal(screen.queryByRole('dialog', { name: 'Needs you' }), null);
});

test('a choice menu marks the current choice; picking another reports it and closes', async () => {
  const user = setup();
  await user.click(screen.getByRole('button', { name: 'prod' }));
  await settle();
  // React Aria names the menu after its trigger, so look it up by role alone.
  assert.ok(screen.getByRole('menu'));
  const current = document.querySelector('[data-od-id="env-prod"]');
  assert.equal(current?.getAttribute('aria-checked'), 'true', 'current choice is marked');
  await user.click(screen.getByRole('menuitemradio', { name: /staging/ }));
  await settle();
  assert.deepEqual(picked, ['staging']);
  assert.equal(screen.queryByRole('menu'), null, 'menu closed after the choice');
  assert.ok(screen.getByRole('button', { name: 'staging' }), 'trigger reflects the new choice');
});

test('the menu is keyboard operable: arrows move, Enter chooses', async () => {
  const user = setup();
  screen.getByRole('button', { name: 'prod' }).focus();
  await user.keyboard('{Enter}');
  await settle();
  await user.keyboard('{ArrowDown}{ArrowDown}{Enter}');
  await settle();
  assert.deepEqual(picked, ['dev']);
});
