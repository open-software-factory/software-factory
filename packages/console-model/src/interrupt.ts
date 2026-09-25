/**
 * A P0 interruption: the factory cannot proceed safely without a decision.
 *
 * The pause is a fact, not a dialog. Minimising the dialog changes what is on
 * screen, never what is paused; only a resume with a recorded justification
 * ends it. Operations (abort, expand containment, run a playbook) are logged
 * against the interruption and leave the pause in force. Nothing here knows
 * about React; the shell maps the state to a Dialog and a critical banner.
 */

export type InterruptStatus = 'none' | 'open' | 'minimized' | 'resolved';

export interface InterruptState {
  status: InterruptStatus;
  justification: string;
  /** Set when resume was attempted without a justification. */
  needsJustification: boolean;
  expanded: boolean;
  playbookRun: boolean;
  aborted: boolean;
  /** Audit trail, oldest first. */
  log: string[];
}

export type InterruptAction =
  | { type: 'raise' }
  | { type: 'minimize' }
  | { type: 'reopen' }
  | { type: 'expand' }
  | { type: 'playbook' }
  | { type: 'abort' }
  | { type: 'justify'; text: string }
  | { type: 'resume' };

export const NO_INTERRUPT: InterruptState = {
  status: 'none', justification: '', needsJustification: false,
  expanded: false, playbookRun: false, aborted: false, log: [],
};

const active = (s: InterruptState) => s.status === 'open' || s.status === 'minimized';
const note = (s: InterruptState, line: string): InterruptState => ({ ...s, log: [...s.log, line] });

export function interruptReduce(state: InterruptState, action: InterruptAction): InterruptState {
  switch (action.type) {
    case 'raise':
      if (active(state)) return { ...state, status: 'open' };
      return note({ ...NO_INTERRUPT, status: 'open' }, 'P0 raised — scope paused at the safe boundary');
    case 'minimize':
      if (state.status !== 'open') return state;
      return note({ ...state, status: 'minimized' }, 'Dialog minimised — pause remains in force');
    case 'reopen':
      if (state.status !== 'minimized') return state;
      return { ...state, status: 'open' };
    case 'expand':
      if (!active(state) || state.expanded) return state;
      return note({ ...state, expanded: true }, 'Containment expanded by operator: full platform delivery pipeline paused');
    case 'playbook':
      if (!active(state) || state.playbookRun) return state;
      return note({ ...state, playbookRun: true }, 'Recovery playbook run: promotion token revoked · SF-138 re-gated');
    case 'abort':
      if (!active(state) || state.aborted) return state;
      return note({ ...state, aborted: true }, 'Affected work aborted: promotion withdrawn · migration step disarmed');
    case 'justify':
      if (!active(state)) return state;
      return { ...state, justification: action.text, needsJustification: state.needsJustification && !action.text.trim() };
    case 'resume': {
      if (!active(state)) return state;
      const text = state.justification.trim();
      if (!text) return { ...state, needsJustification: true };
      return note({ ...state, status: 'resolved', needsJustification: false },
        `P0 resolved — paused scope resumed with recorded justification: “${text}”`);
    }
    default:
      return state;
  }
}

/** The persistent strip shows whenever a pause is in force and the dialog is not covering it. */
export function showsBanner(state: InterruptState): boolean {
  return state.status === 'minimized';
}
