import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { Laboratory, type Session } from './game/Laboratory';
import { Home } from './game/Home';
import './style.css';
function App() { const [session, setSession] = useState<Session | null>(null); return session ? <Laboratory key={session.key} session={session} onExit={() => setSession(null)}/> : <Home onStart={s => setSession({ ...s, key: crypto.randomUUID() })}/>; }
createRoot(document.getElementById('root')!).render(<App />);
