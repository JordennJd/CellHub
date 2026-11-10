import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
type ModalProps = {
    children: ReactNode;
    onClose: () => void;
    label: string;
};
/** A modal owns keyboard focus and temporarily removes the background from navigation. */
export function Modal({ children, onClose, label }: ModalProps) {
    const [host] = useState(() => document.createElement('div'));
    const dialog = useRef<HTMLElement>(null);
    const close = useRef(onClose);
    close.current = onClose;
    useLayoutEffect(() => {
        const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        const previousOverflow = document.body.style.overflow;
        host.setAttribute('data-cellhub-modal', '');
        document.body.appendChild(host);
        const background = new Map<HTMLElement, boolean>();
        for (const sibling of document.body.children) {
            if (sibling instanceof HTMLElement && sibling !== host) {
                background.set(sibling, sibling.inert);
                sibling.inert = true;
            }
        }
        document.body.style.overflow = 'hidden';
        const isTopmost = () => {
            const layers = document.querySelectorAll('[data-cellhub-modal]');
            return layers[layers.length - 1] === host;
        };
        const focusable = () => Array.from(dialog.current?.querySelectorAll<HTMLElement>('button, a[href], input:not([type="hidden"]), select, textarea, [tabindex], [contenteditable="true"]') ?? []).filter(element => element.tabIndex >= 0 && !element.matches(':disabled')
            && !element.closest('[inert]') && element.getClientRects().length > 0);
        const focusFirst = () => (focusable()[0] ?? dialog.current)?.focus();
        if (!dialog.current?.contains(document.activeElement))
            focusFirst();
        const keydown = (event: KeyboardEvent) => {
            if (!isTopmost())
                return;
            if (event.key === 'Escape') {
                event.preventDefault();
                event.stopPropagation();
                close.current();
                return;
            }
            if (event.key !== 'Tab')
                return;
            const elements = focusable();
            const first = elements[0], last = elements[elements.length - 1];
            if (!first) {
                event.preventDefault();
                dialog.current?.focus();
                return;
            }
            if (event.shiftKey && (document.activeElement === first || !elements.includes(document.activeElement as HTMLElement))) {
                event.preventDefault();
                last.focus();
            }
            else if (!event.shiftKey && (document.activeElement === last || !elements.includes(document.activeElement as HTMLElement))) {
                event.preventDefault();
                first.focus();
            }
        };
        const focusin = (event: FocusEvent) => {
            if (isTopmost() && event.target instanceof Node && !dialog.current?.contains(event.target))
                focusFirst();
        };
        document.addEventListener('keydown', keydown, true);
        document.addEventListener('focusin', focusin, true);
        return () => {
            document.removeEventListener('keydown', keydown, true);
            document.removeEventListener('focusin', focusin, true);
            host.remove();
            for (const [element, wasInert] of background)
                if (element.isConnected)
                    element.inert = wasInert;
            document.body.style.overflow = previousOverflow;
            if (previousFocus?.isConnected && !previousFocus.closest('[inert]'))
                previousFocus.focus();
        };
    }, [host]);
    return createPortal(<div className="modal-backdrop"><section ref={dialog} className="modal" role="dialog" aria-modal="true" aria-label={label} tabIndex={-1}>{children}</section></div>, host);
}
