import type { KeyboardEvent } from 'react';

/**
 * onKeyDown for a non-`<button>` element given `role="button"` + `tabIndex={0}`: Enter and
 * Space trigger `action`, like a native button. Keys bubbling up from a nested control are
 * left to that control.
 */
export function activateOnKey(action: () => void) {
  return (e: KeyboardEvent) => {
    if (e.target !== e.currentTarget) {
      return;
    }

    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      action();
    }
  };
}
