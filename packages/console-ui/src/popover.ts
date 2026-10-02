/**
 * Anchored surfaces: a small dialog beside its trigger, or a menu of choices.
 *
 * React Aria positions the surface, flips it near an edge, and owns focus,
 * Escape and outside dismissal. Motion is CSS keyed on its data-entering /
 * data-exiting attributes, which it waits for before unmounting — the same
 * duration and easing tokens the Overlay uses.
 */
import { createElement as h } from 'react';
import type { ReactNode } from 'react';
import { DialogTrigger, Dialog } from 'react-aria-components/Dialog';
import { Popover as RACPopover } from 'react-aria-components/Popover';
import { MenuTrigger, Menu, MenuItem } from 'react-aria-components/Menu';
import { Button } from 'react-aria-components/Button';

export type Placement = 'bottom' | 'bottom start' | 'bottom end' | 'top' | 'top start' | 'top end' | 'left' | 'right';

interface Anchored {
  id: string;
  label: string;
  /** Trigger content; the trigger itself is a React Aria Button. */
  trigger: ReactNode;
  triggerClassName?: string;
  triggerTitle?: string;
  placement?: Placement;
  portalContainer?: Element;
}

export interface PopoverProps extends Anchored {
  children?: ReactNode;
  isOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function Popover({ id, label, trigger, triggerClassName, triggerTitle, placement = 'bottom end',
  portalContainer, children, isOpen, onOpenChange }: PopoverProps) {
  return h(DialogTrigger, { isOpen, onOpenChange },
    h(Button, { className: triggerClassName, title: triggerTitle, 'data-od-id': `${id}-trigger` }, trigger),
    h(RACPopover, {
      placement, offset: 6, className: 'od-popover',
      UNSTABLE_portalContainer: portalContainer,
      'data-od-id': `${id}-popover`,
    }, h(Dialog, { className: 'od-popover-dialog', 'aria-label': label, 'data-od-id': id }, children)));
}

export interface MenuChoice {
  id: string;
  label: string;
  hint?: string;
  /** A colour token expression for a leading dot, e.g. `var(--state-ok)`. */
  tone?: string;
}

export interface ChoiceMenuProps extends Anchored {
  choices: readonly MenuChoice[];
  selected?: string;
  onSelect: (id: string) => void;
}

/** A single-choice menu: the current choice is marked, picking one closes it. */
export function ChoiceMenu({ id, label, trigger, triggerClassName, triggerTitle, placement = 'bottom end',
  portalContainer, choices, selected, onSelect }: ChoiceMenuProps) {
  return h(MenuTrigger, {},
    h(Button, { className: triggerClassName, title: triggerTitle, 'data-od-id': `${id}-trigger` }, trigger),
    h(RACPopover, {
      placement, offset: 6, className: 'od-popover',
      UNSTABLE_portalContainer: portalContainer,
      'data-od-id': `${id}-popover`,
    }, h(Menu, {
      'aria-label': label,
      className: 'od-menu',
      selectionMode: 'single',
      selectedKeys: selected ? [selected] : [],
      onSelectionChange: (keys: unknown) => {
        const first = keys instanceof Set ? [...keys][0] : undefined;
        if (first != null) onSelect(String(first));
      },
      'data-od-id': id,
    }, ...choices.map(c =>
      h(MenuItem, { key: c.id, id: c.id, textValue: c.label, className: 'od-menu-item', 'data-od-id': `${id}-${c.id}` },
        h('span', { className: 'od-menu-mark', 'aria-hidden': 'true' }, '✓'),
        c.tone ? h('i', { className: 'od-menu-dot', style: { background: c.tone } }) : null,
        h('span', { className: 'od-menu-label' }, c.label),
        c.hint ? h('span', { className: 'od-menu-hint' }, c.hint) : null)))));
}
