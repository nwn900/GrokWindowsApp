const {test} = require("node:test");
const assert = require("node:assert/strict");
const vm = require("node:vm");
const fs = require("node:fs");
const path = require("node:path");

function harness(host = "chatgpt.com") {
 let now = 0, text = "Old answer", busy = false, calls = [], timer = [], events = {}, observer;
 const document = {
  readyState: "complete", documentElement: {},
  querySelectorAll: () => [{textContent: text}],
  querySelector: () => busy ? {} : null,
  addEventListener: (name, fn) => { events[name] = fn; }
 };
 const window = {__TAURI__: {core: {invoke: async (command, payload) => calls.push({command,payload})}}};
 window.top = window;
 vm.runInNewContext(fs.readFileSync(path.join(__dirname, "../src-tauri/src/notifications.js"),"utf8"), {
   window, document, location: {hostname:host,pathname:"/chat/1"}, console,
   Date: {now:()=>now},
   MutationObserver: class { constructor(fn){observer=fn;} observe(){} },
   setTimeout: fn => {timer.push(fn);return timer.length;}
 });
 return { calls, events, set(t,b){text=t;busy=b;observer?.();},
   async tick(ms=500){now+=ms; const queued=timer;timer=[];for(const fn of queued) await fn();} };
}
test("history loading never notifies", async () => {
 const h=harness();h.set("Loaded historical conversation",false);
 for(let i=0;i<10;i++)await h.tick();
 assert.equal(h.calls.length,0);
});
test("stream completion notifies exactly once with bounded payload", async () => {
 const h=harness();h.set("Generating",true);await h.tick();
 h.set("a".repeat(500),false);
 for(let i=0;i<20;i++)await h.tick();
 assert.equal(h.calls.length,1);assert.equal(h.calls[0].payload.body.length,300);
 assert.equal(h.calls[0].command,"notify_answer");
});
test("unchanged busy response does not finish early", async () => {
 const h=harness();h.set("Thinking",true);
 for(let i=0;i<20;i++)await h.tick();
 assert.equal(h.calls.length,0);
});
test("auth pages do not install a watcher", async () => {
 const h=harness("accounts.google.com");h.set("Login",false);await h.tick();
 assert.equal(h.calls.length,0);
});
