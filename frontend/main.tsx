import { useLayoutEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { Arena } from './game/Arena';
import { Laboratory, type Session } from './game/Laboratory';
import { Home } from './game/Home';
import './style.css';
function App() {
    const [session, setSession] = useState<Session | null>(null);
    useLayoutEffect(() => { window.scrollTo(0, 0); }, [session?.key]);
    let content = <Home onStart={s => setSession({ ...s, key: crypto.randomUUID() })}/>;
    if (session?.kind === 'arena')
        content = <Arena key={session.key} onExit={() => setSession(null)}/>;
    else if (session)
        content = <Laboratory key={session.key} session={session} onExit={() => setSession(null)}/>;
    return content;
}
createRoot(document.getElementById('root')!).render(<App />);
