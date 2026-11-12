import type { ReactNode } from 'react';
import { Modal } from './Modal';

export function LabPanel({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
    return <Modal label={title} onClose={onClose}>
        <div className="lab-panel-heading"><h2>{title}</h2><button onClick={onClose}>Вернуться к опыту</button></div>
        {children}
    </Modal>;
}
