/**
 * Modal overlay behaviour, independent of shell placement.
 * React Aria owns focus containment/return, outside interaction and Escape.
 * We own the token-driven WAAPI motion and keep the modal mounted until exit
 * finishes. Keeping focus containment during exit prevents click-through.
 *
 * Two contracts share one lifecycle:
 *   modal      drawers, dialogs, the palette — scrim, focus containment,
 *              outside-dismiss, Escape anywhere (React Aria owns these)
 *   non-modal  the floating inspector — no scrim, no focus capture, content
 *              stays clickable, Escape only from inside the panel
 */
import { createElement as h, useLayoutEffect, useRef, useState } from 'react';
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent, ReactNode, RefObject } from 'react';
import { createPortal } from 'react-dom';
import { Modal, ModalOverlay } from 'react-aria-components/Modal';
import { Dialog } from 'react-aria-components/Dialog';

export interface OverlayProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  label: string;
  id: string;
  children?: ReactNode;
  placement?: 'left' | 'right' | 'center' | 'top' | 'bottom';
  /** Default true. False: floats over content without a scrim or focus trap. */
  modal?: boolean;
  /** Inline style on the panel — a floating surface's dragged offset, say. */
  panelStyle?: CSSProperties;
  /** `alertdialog` for a decision the operator must make before continuing. */
  role?: 'dialog' | 'alertdialog';
  /** Extra class on the panel, e.g. a width variant. */
  className?: string;
  portalContainer?: Element;
  isDismissable?: boolean;
  isKeyboardDismissDisabled?: boolean;
  triggerRef?: RefObject<HTMLElement | null>;
  /** When the original trigger disappears (e.g. a breakpoint change). */
  fallbackFocusRef?: RefObject<HTMLElement | null>;
}

function Surface({ isOpen, onExited, placement, label, id, children, backdropRef, modal, onEscape, role, className, panelStyle }: {
  isOpen: boolean;
  onExited: () => void;
  placement: NonNullable<OverlayProps['placement']>;
  label: string;
  id: string;
  children?: ReactNode;
  backdropRef: RefObject<HTMLDivElement | null>;
  modal: boolean;
  onEscape?: () => void;
  role?: 'dialog' | 'alertdialog';
  className?: string;
  panelStyle?: CSSProperties;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const previous = useRef<{ opacity: string; transform: string } | null>(null);
  const exitedRef = useRef(onExited);
  exitedRef.current = onExited;

  useLayoutEffect(() => {
    const panel = panelRef.current;
    // Parent DOM refs are attached after child layout effects on first mount.
    // The ancestor already exists in the portal even before its ref is set.
    const backdrop = backdropRef.current ?? panel?.closest('.od-overlay-backdrop, .od-overlay-float');
    if (!panel || !backdrop) return;
    const css = getComputedStyle(panel);
    const token = css.getPropertyValue('--dur-panel').trim();
    const duration = token.endsWith('ms') ? parseFloat(token) : parseFloat(token) * 1000;
    const options = {
      duration: Number.isFinite(duration) ? duration : 200,
      easing: css.getPropertyValue('--ease').trim() || 'cubic-bezier(0.2,0,0.2,1)',
      fill: 'both' as const,
    };
    const offset = placement === 'left' ? 'translateX(-100%)'
      : placement === 'right' ? 'translateX(100%)'
      : placement === 'bottom' ? 'translateY(100%)'
      : placement === 'top' ? 'translateY(-12px)' : 'translateY(8px)';
    const from = previous.current ?? { opacity: isOpen ? '0' : '1', transform: isOpen ? offset : 'none' };
    const to = { opacity: isOpen ? '1' : '0', transform: isOpen ? 'none' : offset };
    let disposed = false;
    panel.dataset.phase = isOpen ? 'entering' : 'exiting';
    const motion = panel.animate([from, to], options);
    const fade = backdrop.animate([{ opacity: from.opacity }, { opacity: to.opacity }], options);
    Promise.all([motion.finished, fade.finished]).then(() => {
      if (disposed) return;
      if (isOpen) panel.dataset.phase = 'open';
      else exitedRef.current();
    }).catch(() => { /* Cancellation is expected on reversal/unmount. */ });
    return () => {
      disposed = true;
      const current = getComputedStyle(panel);
      previous.current = { opacity: current.opacity, transform: current.transform };
      motion.cancel();
      fade.cancel();
    };
  }, [isOpen, placement, backdropRef]);

  if (!modal) {
    // A non-modal dialog: labelled, reachable, never trapping. Escape counts
    // only when focus is inside, so content shortcuts keep working.
    return h('div', {
      ref: panelRef,
      className: `od-overlay-panel od-overlay-${placement}${className ? ' ' + className : ''}`,
      style: panelStyle,
      'data-od-id': `${id}-panel`,
      onKeyDown: (e: ReactKeyboardEvent) => {
        if (e.key === 'Escape' && onEscape) { e.preventDefault(); e.stopPropagation(); onEscape(); }
      },
    }, h('div', {
      id,
      role: 'dialog',
      className: 'od-overlay-dialog',
      'aria-label': label,
      'data-od-id': id,
    }, children));
  }
  return h(Modal, {
    ref: panelRef,
    className: `od-overlay-panel od-overlay-${placement}${className ? ' ' + className : ''}`,
    style: panelStyle,
    'data-od-id': `${id}-panel`,
  }, h(Dialog, {
    id,
    role,
    className: 'od-overlay-dialog',
    'aria-label': label,
    'data-od-id': id,
  }, children));
}

export function Overlay({ isOpen, onOpenChange, label, id, children,
  placement = 'center', modal = true, portalContainer, isDismissable = true,
  isKeyboardDismissDisabled = false, triggerRef, fallbackFocusRef, role, className, panelStyle }: OverlayProps) {
  const [present, setPresent] = useState(isOpen);
  const backdropRef = useRef<HTMLDivElement>(null);
  const returnTarget = useRef<Element | null>(null);
  const wasOpen = useRef(false);
  useLayoutEffect(() => {
    if (isOpen) {
      if (!wasOpen.current) returnTarget.current = triggerRef?.current ?? null;
      setPresent(true);
    }
    wasOpen.current = isOpen;
  }, [isOpen, triggerRef]);

  if (!isOpen && !present) return null;
  if (!modal) {
    const surface = h('div', {
      ref: backdropRef,
      className: `od-overlay-float od-overlay-align-${placement}`,
      'data-od-id': `${id}-backdrop`,
    }, h(Surface, {
      isOpen, label, id, placement, backdropRef, modal: false, className, panelStyle,
      onEscape: isKeyboardDismissDisabled ? undefined : () => onOpenChange(false),
      onExited: () => {
        // Only a panel that held focus needs to hand it somewhere on close;
        // decide before the unmount moves focus to the body.
        const held = !!backdropRef.current?.contains(document.activeElement);
        setPresent(false);
        if (held) {
          const target = returnTarget.current?.isConnected ? returnTarget.current as HTMLElement : fallbackFocusRef?.current;
          requestAnimationFrame(() => target?.focus({ preventScroll: true }));
        }
      },
    }, children));
    return createPortal(surface, portalContainer ?? document.body);
  }
  return h(ModalOverlay, {
    ref: backdropRef,
    isOpen: true,
    onOpenChange,
    isDismissable,
    isKeyboardDismissDisabled,
    // Kept at this one adapter boundary; the application never uses RAC's
    // portal API. The stage container keeps overlays inside the preview.
    UNSTABLE_portalContainer: portalContainer,
    className: `od-overlay-backdrop od-overlay-align-${placement}`,
    'data-od-id': `${id}-backdrop`,
  }, h(Surface, {
    isOpen, label, id, placement, backdropRef, modal: true, role, className, panelStyle,
    onExited: () => {
      setPresent(false);
      if (returnTarget.current && !returnTarget.current.isConnected) {
        requestAnimationFrame(() => fallbackFocusRef?.current?.focus({ preventScroll: true }));
      }
    },
  }, children));
}
