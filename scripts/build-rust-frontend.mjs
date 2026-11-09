import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
const exe=process.platform==='win32'?'.exe':'';
const simulation=process.argv.includes('--simulation');
const crate=simulation?'cellhub-simulation':'cellhub-frontend';
const stem=simulation?'cellhub_simulation':'cellhub_frontend';
const output=simulation?'frontend/wasm':'frontend/rust-wasm';
const name=simulation?'simulation':'cellhub_frontend';
const localCargo=join(homedir(),'.cargo','bin',`cargo${exe}`);
const cargo=existsSync(localCargo)?localCargo:'cargo';
const localBindgen=join(process.cwd(),'.tools','wasm-bindgen','bin',`wasm-bindgen${exe}`);
const bindgen=existsSync(localBindgen)?localBindgen:'wasm-bindgen';
for(const [program,args] of [[cargo,['build','-p',crate,'--release','--target','wasm32-unknown-unknown']],[bindgen,[`target/wasm32-unknown-unknown/release/${stem}.wasm`,'--target','web','--out-dir',output,'--out-name',name]]]) {
 const r=spawnSync(program,args,{stdio:'inherit'});if(r.error)throw r.error;if(r.status!==0)process.exit(r.status??1);
}
