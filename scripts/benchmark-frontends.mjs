import { chromium } from 'playwright';
import { spawn } from 'node:child_process';
const server=spawn(process.execPath,['node_modules/vite/bin/vite.js','--host','127.0.0.1','--port','5188','--strictPort'],{windowsHide:true,stdio:'pipe'});
await new Promise((resolve,reject)=>{server.stdout.on('data',d=>{if(String(d).includes('5188'))resolve();});server.on('error',reject);server.on('exit',code=>reject(new Error(`Vite exited ${code}`)));});
const browser=await chromium.launch({channel:'chrome',headless:true,args:['--no-proxy-server']});
try {
 const page=await browser.newPage({viewport:{width:1440,height:1000},deviceScaleFactor:1});
 const errors=[];page.on('pageerror',e=>errors.push(String(e)));
 for(const count of [1000,5000]) for(const paused of [true,false]) for(const variant of ['webgl','webgpu','rust']) {
  errors.length=0;
  const url=`http://127.0.0.1:5188/${variant==='rust'?'rust.html':'legacy.html'}?count=${count}${paused?'&paused':''}${variant==='webgpu'?'&webgpu':''}`;
  await page.goto(url);
  if(variant==='rust') await page.waitForFunction(()=>window.cellhubReady || document.querySelector('#error')?.textContent,{},{timeout:120000});
  else await page.waitForSelector('canvas');
  await page.waitForTimeout(5000);
  const error=await page.evaluate(()=>document.querySelector('#error')?.textContent);
  if(error) { console.log(JSON.stringify({variant,count,paused,error}));continue; }
  const result=await page.evaluate(()=>new Promise(resolve=>{const intervals=[];let last=0,start=0;function tick(t){if(!start)start=t;if(last)intervals.push(t-last);last=t;if(t-start<10000)requestAnimationFrame(tick);else {intervals.sort((a,b)=>a-b);resolve({fps:1000/(intervals.reduce((a,b)=>a+b,0)/intervals.length),p95ms:intervals[Math.floor(intervals.length*.95)]});}}requestAnimationFrame(tick);}));
  const adapter=await page.evaluate(async()=>{const a=await navigator.gpu?.requestAdapter();return a?{...a.info.toJSON?.(),vendor:a.info.vendor,architecture:a.info.architecture,description:a.info.description}:null;});
  console.log(JSON.stringify({variant,count,paused,...result,adapter,errors:[...errors]}));
  if(variant==='rust'&&count===1000&&paused) await page.screenshot({path:'rust-smoke.png'});
 }
 await page.goto('http://127.0.0.1:5188/?paused');
 await page.waitForFunction(()=>window.cellhubReady);
 await page.waitForTimeout(1000);
 await page.mouse.click(400,400);
 await page.mouse.click(1168,248);
 await page.waitForTimeout(500);
 const saved=await page.evaluate(()=>JSON.parse(localStorage.getItem('cellhub.rust.genome.v1')||'null'));
 if(!Array.isArray(saved)||saved.length!==64) throw new Error('Save genome smoke test failed');
 await page.screenshot({path:'rust-smoke.png'});
 console.log(JSON.stringify({smoke:'spawn click + save 64 genome values',errors}));
} finally {await browser.close();server.kill();}
