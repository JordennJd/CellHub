import init, { start } from './rust-wasm/cellhub_frontend.js';
try {
 if (!navigator.gpu) throw new Error('WebGPU недоступен. Используйте совместимый браузер на localhost или HTTPS.');
 const q = new URLSearchParams(location.search);
 await init();
 await start(document.getElementById('canvas'), Math.min(5000, Math.max(0, Number(q.get('count')) || 0)), q.has('paused'));
 window.cellhubReady = true;
} catch (error) {
 document.getElementById('error').textContent = String(error);
 console.error(error);
}
