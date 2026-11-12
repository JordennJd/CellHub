import { useEffect, useId, useRef, useState } from 'react';
import './simulation-header.css';

export function SimulationHeader({ title, onExit, action }: {
    title: string;
    onExit: () => void;
    action: { label: string; run: () => void; disabled?: boolean };
}) {
    const actionsId = useId();
    const header = useRef<HTMLElement>(null);
    const actions = useRef<HTMLDivElement>(null);
    const toggle = useRef<HTMLButtonElement>(null);
    const [open, setOpen] = useState(false);

    useEffect(() => {
        if (!open) return;
        const dismissOutside = (event: PointerEvent) => {
            if (event.target instanceof Node && !header.current?.contains(event.target)) setOpen(false);
        };
        const dismissOnEscape = (event: KeyboardEvent) => {
            if (event.key !== 'Escape' || event.defaultPrevented) return;
            if (actions.current?.contains(document.activeElement)) toggle.current?.focus();
            setOpen(false);
        };
        document.addEventListener('pointerdown', dismissOutside);
        document.addEventListener('keydown', dismissOnEscape);
        return () => {
            document.removeEventListener('pointerdown', dismissOutside);
            document.removeEventListener('keydown', dismissOnEscape);
        };
    }, [open]);

    function runAction(run: () => void) {
        // A dialog opened by this action can restore focus to the visible toggle.
        toggle.current?.focus();
        setOpen(false);
        run();
    }

    return <header ref={header} className="simulation-header"
        onPointerEnter={event => { if (event.pointerType === 'mouse') setOpen(true); }}
        onPointerLeave={event => {
            if (event.pointerType === 'mouse' && !event.currentTarget.querySelector(':focus-visible')) setOpen(false);
        }}
        onBlur={event => {
            if (event.relatedTarget && !event.currentTarget.contains(event.relatedTarget)) setOpen(false);
        }}>
        <h1 title={title}>{title}</h1>
        <button ref={toggle} type="button" className="simulation-header-toggle" aria-expanded={open}
            aria-controls={actionsId} onClick={() => setOpen(value => !value)}>Действия</button>
        <div ref={actions} id={actionsId} className="simulation-header-actions" hidden={!open}>
            <button type="button" onClick={() => runAction(onExit)}>← Меню</button>
            <button type="button" disabled={action.disabled} onClick={() => runAction(action.run)}>{action.label}</button>
        </div>
    </header>;
}
