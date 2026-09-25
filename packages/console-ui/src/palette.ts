/**
 * Command palette: Overlay + search field + filtered list.
 *
 * React Aria's Autocomplete keeps focus in the field and moves a virtual
 * cursor through the list, so typing and arrowing never fight. The list is
 * ordered by the tested filter in shell/commands.ts; this file only draws it.
 * Running a command closes through the shared Overlay lifecycle, so focus
 * returns to the trigger the same way the drawer's does.
 */
import { createElement as h, useEffect, useMemo, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent, RefObject } from 'react';
import { Autocomplete } from 'react-aria-components/Autocomplete';
import { SearchField, Input } from 'react-aria-components/SearchField';
import { ListBox, ListBoxItem, ListBoxSection, Header, Text } from 'react-aria-components/ListBox';
import { Overlay } from './overlay.ts';
import { filterCommands, groupCommands, isPaletteShortcut } from '../../console-model/src/commands.ts';
import type { Command } from '../../console-model/src/commands.ts';

export interface CommandPaletteProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  commands: readonly Command[];
  onRun: (command: Command) => void;
  portalContainer?: Element;
  triggerRef?: RefObject<HTMLElement | null>;
  placeholder?: string;
  id?: string;
}

/** Ctrl+P / ⌘K anywhere in the window toggles the palette. */
export function usePaletteShortcut(onToggle: () => void, enabled = true): void {
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || !isPaletteShortcut(e)) return;
      e.preventDefault();
      // Consumed here: Autocomplete re-dispatches unhandled keys to its list,
      // and that copy would reach this listener a second time.
      e.stopPropagation();
      onToggle();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [onToggle, enabled]);
}

export function CommandPalette({ isOpen, onOpenChange, commands, onRun, portalContainer, triggerRef,
  placeholder = 'Search work, runs, panels, actions', id = 'command-palette' }: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  // A reopened palette starts blank; the last query is not a state worth keeping.
  useEffect(() => { if (isOpen) setQuery(''); }, [isOpen]);

  const groups = useMemo(() => groupCommands(filterCommands(commands, query)), [commands, query]);
  const byId = useMemo(() => new Map(commands.map(c => [c.id, c])), [commands]);

  const run = (key: unknown) => {
    const command = byId.get(String(key));
    if (!command) return;
    onOpenChange(false);
    onRun(command);
  };
  // Escape closes outright: a two-step "clear, then close" hides the exit.
  const onKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key === 'Escape') { e.preventDefault(); onOpenChange(false); }
  };

  return h(Overlay, { isOpen, onOpenChange, id, label: 'Command palette', placement: 'top', portalContainer, triggerRef },
    h(Autocomplete, { inputValue: query, onInputChange: setQuery },
      h(SearchField, { 'aria-label': 'Search commands', className: 'od-palette-field' },
        h('span', { className: 'od-palette-glyph', 'aria-hidden': 'true' }, '⌕'),
        h(Input, { className: 'od-palette-input', placeholder, autoFocus: true, onKeyDown,
          'data-od-id': `${id}-input` })),
      h(ListBox, {
        'aria-label': 'Commands',
        className: 'od-palette-list',
        selectionMode: 'none',
        onAction: run,
        'data-od-id': `${id}-list`,
        renderEmptyState: () => h('div', { className: 'od-palette-empty' }, `Nothing matches “${query.trim()}”.`),
        items: groups,
      }, (group: { group: string; items: Command[] }) =>
        h(ListBoxSection, { id: group.group, className: 'od-palette-group' },
          h(Header, { className: 'od-palette-heading' }, group.group),
          ...group.items.map(c =>
            h(ListBoxItem, { key: c.id, id: c.id, textValue: c.label, className: 'od-palette-row', 'data-od-id': c.id },
              h(Text, { slot: 'label', className: 'od-palette-label' }, c.label),
              c.hint ? h('span', { className: 'od-palette-hint' }, c.hint) : null))))),
    h('div', { className: 'od-palette-foot', 'aria-hidden': 'true' },
      h('span', {}, h('kbd', {}, '↑↓'), ' move'),
      h('span', {}, h('kbd', {}, '↵'), ' run'),
      h('span', {}, h('kbd', {}, 'esc'), ' close')));
}
