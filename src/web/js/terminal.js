/* A terminal that behaves like one: the command types, the output arrives in its
   own time, the cursor blinks, and the window controls do what they say. */
(function () {
  const term = document.getElementById('terminal');
  const body = document.getElementById('terminal-body');
  if (!term || !body) return;

  const PROMPT = '<span class="terminal__user">sherlock</span><span class="terminal__host">@webleads</span>'
               + '<span class="terminal__path">:~/Projects/webleads</span><span class="terminal__sigil">$ </span>';

  const SESSION = [
    { cmd: 'ls out/', out: [
      '<span class="is-dim">00-BUDAPEST-ZUGLO-Hosok_tere.csv</span>',
      '<span class="is-dim">00-BUDAPEST-ZUGLO-Hosok_tere.html</span>',
      '<span class="is-dim">queries.txt</span>',
    ]},
    { cmd: './bin/gmaps -input out/queries.txt -results out/leads.csv -lang hu -depth 10 -c 2',
      out: [
      '<span class="is-dim">23:24:01</span> <span class="is-accent">INFO</span>  starting scrapemate',
      '<span class="is-dim">23:24:04</span> <span class="is-accent">INFO</span>  20 places found   <span class="is-dim">vízvezeték-szerelő Budapest Zugló</span>',
      '<span class="is-dim">23:24:31</span> <span class="is-accent">INFO</span>  18 places found   <span class="is-dim">villanyszerelő Budapest Zugló</span>',
      '<span class="is-dim">23:25:02</span> <span class="is-accent">INFO</span>  11 places found   <span class="is-dim">tetőfedő Budapest Zugló</span>',
      '<span class="is-dim">23:25:38</span> <span class="is-warn">WARN</span>  google refused 2 searches, slowing down',
      '<span class="is-dim">23:26:10</span> <span class="is-accent">INFO</span>  24 places found   <span class="is-dim">autószerelő Budapest Zugló</span>',
      '<span class="is-accent">[████████████░░░░░░░░]</span>  12/27 words   <span class="is-ok">318</span> businesses',
      '<span class="is-bad">^C</span> <span class="is-dim">stopped by hand — partial results kept</span>',
    ], pace: 260 },
    { cmd: 'python3 scripts/filter_leads.py out/leads.csv', out: [
      'scanned      <span class="is-ok">318</span>',
      'in area      <span class="is-ok">271</span>   <span class="is-dim">(2000 m of Hősök tere)</span>',
      'duplicates   <span class="is-dim">47</span>',
      'no website   <span class="is-ok">33</span>',
      'social only  <span class="is-warn">5</span>',
      'dead site    <span class="is-bad">3</span>',
      '<span class="is-accent">──────────────────────────</span>',
      '<span class="is-ok">41 leads</span> <span class="is-dim">worth telephoning</span>',
    ]},
    { cmd: 'head -3 out/00-BUDAPEST-ZUGLO-Hosok_tere.csv', out: [
      '<span class="is-dim">Name,Phone,Email,Created,Maps URL,Website,Social URL,Why</span>',
      'Fodrászat,<span class="is-accent">06205550101</span>,,,https://…,,,<span class="is-ok">no_site</span>',
      'Petra’z Hair Zugló,<span class="is-accent">06305550102</span>,,,https://…,,,<span class="is-ok">no_site</span>',
    ]},
    { cmd: "curl -s localhost:8080/api/v1/jobs | jq -r '.[].Status'", out: [
      '<span class="is-ok">ok</span>',
    ]},
    { cmd: './bin/gmaps --deduce', out: [
      '<span class="is-dim">“You see, but you do not observe.”</span>',
      'Forty-one telephones on this street. <span class="is-accent">Not one website.</span>',
    ]},
  ];

  const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  let timer = null, stopped = false;

  const line = html => {
    const el = document.createElement('span');
    el.className = 'terminal__line';
    el.innerHTML = html;
    body.appendChild(el);
    body.scrollTop = body.scrollHeight;
    return el;
  };
  const wait = ms => new Promise(done => { timer = setTimeout(done, ms); });

  async function type(el, text) {
    for (let i = 1; i <= text.length; i++) {
      if (stopped) return;
      el.innerHTML = PROMPT + '<span class="terminal__cmd">' + text.slice(0, i)
                   + '</span><span class="terminal__cursor"></span>';
      body.scrollTop = body.scrollHeight;
      await wait(text[i - 1] === ' ' ? 26 : 15 + Math.floor(i % 3) * 6);
    }
    el.innerHTML = PROMPT + '<span class="terminal__cmd">' + text + '</span>';
  }

  async function run() {
    body.textContent = '';
    if (still) {
      for (const step of SESSION) {
        line(PROMPT + '<span class="terminal__cmd">' + step.cmd + '</span>');
        step.out.forEach(line);
        line('');
      }
      line(PROMPT + '<span class="terminal__cursor"></span>');
      return;
    }
    for (const step of SESSION) {
      if (stopped) return;
      await wait(420);
      await type(line(PROMPT + '<span class="terminal__cursor"></span>'), step.cmd);
      await wait(300);
      for (const out of step.out) {
        if (stopped) return;
        line(out);
        await wait(step.pace ?? 130);
      }
      line('');
    }
    if (stopped) return;
    line(PROMPT + '<span class="terminal__cursor"></span>');
    await wait(4200);
    if (!stopped) run();
  }

  term.querySelector('.terminal__controls').addEventListener('click', event => {
    const act = event.target.closest('button')?.dataset.act;
    if (act === 'roll') term.dataset.state = term.dataset.state === 'rolled' ? 'open' : 'rolled';
    if (act === 'tall') term.dataset.state = term.dataset.state === 'tall' ? 'open' : 'tall';
    if (act === 'end') {
      stopped = true; clearTimeout(timer);
      line('');
      line('<span class="is-dim">[process completed — click inside to start a new session]</span>');
    }
  });
  body.addEventListener('click', () => {
    if (!stopped) return;
    stopped = false; run();
  });

  run();
  console.log('%c TERMINAL %c sherlock@webleads session running',
    'background:#8a6820;color:#0b0a08;font-weight:700;padding:1px 6px', 'color:#8a6820');
})();
