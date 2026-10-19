/* Prism background — a beam enters a prism on the left and fans out across the page.
   WebGPU when the browser has it; a plain CSS wash when it does not, and it says
   which one you are looking at rather than failing quietly. */
(async function () {
  const canvas = document.getElementById('prism-canvas');
  if (!canvas) return;

  // No switch on the page. `localStorage.prism-off = "1"` turns it off for anybody
  // who wants it off, and the reduced-motion setting is honoured below whatever this
  // says.
  const KEY = 'prism-off';
  if (localStorage.getItem(KEY) === '1') {
    canvas.style.display = 'none';
    return;
  }
  canvas.style.display = 'block';
  const badge = (msg, ok) => console.log('%c PRISM %c ' + msg,
    'background:' + (ok ? '#8a6820' : '#6b6459') + ';color:#0b0a08;font-weight:700;padding:1px 6px',
    'color:#8a6820');

  if (!navigator.gpu) {
    document.body.classList.add('is-prism-css');
    badge('WebGPU not available in this browser — using the CSS wash instead', false);
    return;
  }
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) {
    document.body.classList.add('is-prism-css');
    badge('no WebGPU adapter — using the CSS wash instead', false);
    return;
  }
  const device = await adapter.requestDevice();
  const ctx = canvas.getContext('webgpu');
  const format = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format, alphaMode: 'premultiplied' });

  const shader = device.createShaderModule({ code: `
struct U { res: vec2f, time: f32, track: f32, mouse: vec2f, pad: vec2f };
@group(0) @binding(0) var<uniform> u: U;

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
  var p = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  return vec4f(p[i], 0.0, 1.0);
}

// distance from p to the ray that starts at o and runs along d
fn rayDist(p: vec2f, o: vec2f, d: vec2f) -> vec2f {
  let v = p - o;
  let t = dot(v, d);
  return vec2f(length(v - d * max(t, 0.0)), t);
}

fn seg(p: vec2f, a: vec2f, b: vec2f) -> f32 {
  let pa = p - a; let ba = b - a;
  let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  return length(pa - ba * h);
}

// signed distance to a triangle: negative inside
fn sdTri(p: vec2f, p0: vec2f, p1: vec2f, p2: vec2f) -> f32 {
  let e0 = p1 - p0; let e1 = p2 - p1; let e2 = p0 - p2;
  let v0 = p - p0;  let v1 = p - p1;  let v2 = p - p2;
  let q0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
  let q1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
  let q2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
  let s = sign(e0.x * e2.y - e0.y * e2.x);
  let d = min(min(vec2f(dot(q0, q0), s * (v0.x * e0.y - v0.y * e0.x)),
                  vec2f(dot(q1, q1), s * (v1.x * e1.y - v1.y * e1.x))),
                  vec2f(dot(q2, q2), s * (v2.x * e2.y - v2.y * e2.x)));
  return -sqrt(d.x) * sign(d.y);
}

@fragment fn fs(@builtin(position) fc: vec4f) -> @location(0) vec4f {
  let uv = fc.xy / u.res;
  let a  = u.res.x / max(u.res.y, 1.0);
  let p  = vec2f(uv.x * a, uv.y);

  // The prism never moves. Nine tenths of the header tall, equilateral, and whole:
  // a hair in from the left edge rather than pushed off it.
  let hgt  = 0.90;
  let wid  = hgt * 1.1547;
  let midY = 0.5;
  let leftX = 0.03;
  let vL = vec2f(leftX,            midY + hgt * 0.5);
  let vR = vec2f(leftX + wid,      midY + hgt * 0.5);
  let vA = vec2f(leftX + wid * 0.5, midY - hgt * 0.5);

  // the light follows the pointer; when there is none it sweeps on its own
  let exitPt = mix(vA, vR, 0.46);
  let mp = vec2f(u.mouse.x / max(u.res.x, 1.0) * a, u.mouse.y / max(u.res.y, 1.0));
  let toM = mp - exitPt;
  let aim = clamp(toM.y / max(toM.x, 0.09), -1.15, 1.15);
  let sweep = sin(u.time * 0.26) * 0.62 + sin(u.time * 0.09) * 0.10;
  let tilt = mix(sweep, aim, u.track);

  let dTri = sdTri(p, vL, vA, vR);
  let inside = 1.0 - smoothstep(-0.006, 0.006, dTri);
  let edge   = 1.0 - smoothstep(0.0, 0.0045, abs(dTri));

  // the beam arrives at the middle of the left face
  let hit = mix(vL, vA, 0.78);
  let dirIn = normalize(vec2f(1.0, 0.42 + tilt * 0.45));
  let src = hit - dirIn * 2.4;
  let inbeam = 1.0 - smoothstep(0.0, 0.0075, seg(p, src, hit));

  // through the glass, bent, and out of the right face
  let exit = exitPt;
  let through = 1.0 - smoothstep(0.0, 0.0065, seg(p, hit, exit));
  var col = vec3f(0.0);
  let N = 8;
  for (var i = 0; i < N; i = i + 1) {
    let k = f32(i) / f32(N - 1);
    let ang = mix(tilt * 0.60, tilt, u.track) - 0.27 + 0.54 * k;
    let dir = normalize(vec2f(1.0, ang));
    let rd  = rayDist(p, exit, dir);
    let wide = 0.003 + rd.y * 0.030;
    let core = exp(-(rd.x * rd.x) / (2.0 * wide * wide));
    let reach = exp(-rd.y * 0.82) * step(0.0, rd.y);
    let c1 = vec3f(0.541, 0.408, 0.125);
    let c2 = vec3f(0.788, 0.588, 0.184);
    let c3 = vec3f(0.910, 0.886, 0.831);
    var tint = mix(c1, c2, smoothstep(0.0, 0.55, k));
    tint = mix(tint, c3, smoothstep(0.55, 1.0, k));
    col += tint * core * reach * 0.30;
  }

  col += vec3f(0.910, 0.886, 0.831) * inbeam * 0.50;
  col += vec3f(0.860, 0.760, 0.520) * through * 0.30;
  col += vec3f(0.788, 0.588, 0.184) * edge * 0.42;
  col += vec3f(0.541, 0.408, 0.125) * inside * 0.045;
  col += vec3f(0.910, 0.886, 0.831) * exp(-length(p - exit) * 13.0) * 0.14;

  col = min(col, vec3f(0.80));
  let alpha = clamp(max(col.r, max(col.g, col.b)), 0.0, 1.0);
  return vec4f(col * alpha, alpha);
}
` });

  const buffer = device.createBuffer({
    size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });

  // where the pointer is, where the light currently points, and how much it cares
  const want = { x: 0, y: 0, track: 0 };
  const have = { x: 0, y: 0, track: 0 };
  let seeded = false;
  window.addEventListener('pointermove', event => {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    // Measured from the canvas, which sits at the top of the page and scrolls with it.
    const at = canvas.getBoundingClientRect();
    want.x = (event.clientX - at.left) * dpr;
    want.y = (event.clientY - at.top) * dpr;
    want.track = 1;
    if (!seeded) { have.x = want.x; have.y = want.y; seeded = true; }
  }, { passive: true });
  const release = () => { want.track = 0; };
  window.addEventListener('pointerleave', release);
  window.addEventListener('blur', release);
  const pipeline = device.createRenderPipeline({
    layout: 'auto',
    vertex: { module: shader, entryPoint: 'vs' },
    fragment: { module: shader, entryPoint: 'fs', targets: [{
      format,
      blend: { color: { srcFactor: 'one', dstFactor: 'one' },
               alpha: { srcFactor: 'one', dstFactor: 'one' } },
    }] },
    primitive: { topology: 'triangle-list' },
  });
  const bind = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [{ binding: 0, resource: { buffer } }],
  });

  function resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.floor(canvas.clientWidth * dpr);
    canvas.height = Math.floor(canvas.clientHeight * dpr);
  }
  window.addEventListener('resize', resize);
  resize();

  const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const start = performance.now();
  function frame(now) {
    if (!canvas.width || !canvas.height) { requestAnimationFrame(frame); return; }
    const t = still ? 2.0 : (now - start) / 1000;
    // ease towards the pointer so the light glides instead of snapping
    const ease = still ? 1 : 0.085;
    have.x += (want.x - have.x) * ease;
    have.y += (want.y - have.y) * ease;
    have.track += (want.track - have.track) * (still ? 1 : 0.05);
    device.queue.writeBuffer(buffer, 0, new Float32Array(
      [canvas.width, canvas.height, t, have.track, have.x, have.y, 0, 0]));
    const pass = device.createCommandEncoder();
    const enc = pass.beginRenderPass({ colorAttachments: [{
      view: ctx.getCurrentTexture().createView(),
      clearValue: { r: 0, g: 0, b: 0, a: 0 },
      loadOp: 'clear', storeOp: 'store' }] });
    enc.setPipeline(pipeline);
    enc.setBindGroup(0, bind);
    enc.draw(3);
    enc.end();
    device.queue.submit([pass.finish()]);
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
  badge('WebGPU running' + (still ? ' (motion reduced — holding still)' : ''), true);
})();
