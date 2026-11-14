// Ponte do SBF dentro da página (injetada pelo CDP em todo documento novo).
// 1. Barra clique e envio perigosos na fase de captura (RO-02), com a MESMA lista do executor.
// 2. Descreve um elemento para o executor conferir a lista antes de agir.
// 3. Entrega a página simplificada para a IA: texto visível, elementos clicáveis com seletor e
//    formulários. NUNCA devolve o valor de nenhum campo (RO-01): só se está preenchido ou não.
(function () {
  if (window.__sbfPonte) return;
  const L = __SBF_LISTA__;
  const LIM = L.limite || 300;

  function palavras(s) {
    s = String(s == null ? '' : s).slice(0, LIM).replace(/([a-zà-ÿ])([A-Z])/g, '$1 $2');
    s = s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
    return s.split(/[^a-z0-9]+/).filter(Boolean);
  }
  function termo(valor) {
    const p = palavras(valor);
    for (const w of p) {
      for (const t of L.contem) if (w.indexOf(t) !== -1) return t;
      for (const t of L.palavra) if (w === t) return t;
    }
    for (const f of L.frases) {
      const a = f.split(' ');
      for (let i = 0; i + a.length <= p.length; i++) {
        let ok = true;
        for (let j = 0; j < a.length; j++) if (p[i + j] !== a[j]) { ok = false; break; }
        if (ok) return f;
      }
    }
    return null;
  }

  const INTERATIVOS = 'a,button,input,select,summary,label,[role=button],[role=link],[role=menuitem],[onclick]';
  function eSenha(el) { return el && el.tagName === 'INPUT' && String(el.type).toLowerCase() === 'password'; }

  function descrever(el) {
    const campos = [];
    const alvo = (el.closest && el.closest(INTERATIVOS)) || el;
    const add = (n, v) => { if (v != null && String(v).trim() !== '') campos.push([n, String(v).slice(0, LIM)]); };
    if (!eSenha(alvo)) {
      add('texto', alvo.innerText || alvo.textContent);
      add('value', alvo.value);
    }
    add('aria-label', alvo.getAttribute && alvo.getAttribute('aria-label'));
    add('title', alvo.getAttribute && alvo.getAttribute('title'));
    add('alt', alvo.getAttribute && alvo.getAttribute('alt'));
    add('name', alvo.getAttribute && alvo.getAttribute('name'));
    add('id', alvo.id);
    add('destino', alvo.getAttribute && (alvo.getAttribute('href') || alvo.getAttribute('formaction')));
    const img = alvo.querySelector && alvo.querySelector('img[alt]');
    if (img) add('alt', img.getAttribute('alt'));
    const f = alvo.form || (alvo.closest && alvo.closest('form'));
    const t = String(alvo.type || '').toLowerCase();
    if (f && (t === 'submit' || t === 'image' || (alvo.tagName === 'BUTTON' && t !== 'button' && t !== 'reset'))) {
      add('destino', f.getAttribute('action'));
      add('id', f.id);
      add('name', f.getAttribute('name'));
    }
    return { campos: campos, eh_senha: !!eSenha(alvo), eh_campo: /^(INPUT|TEXTAREA|SELECT)$/.test(alvo.tagName) };
  }
  function formularioPorSubmit(form, submitter) {
    const campos = [];
    const add = (n, v) => { if (v != null && String(v).trim() !== '') campos.push([n, String(v).slice(0, LIM)]); };
    add('destino', form.getAttribute('action')); add('id', form.id); add('name', form.getAttribute('name'));
    if (submitter) {
      const d = descrever(submitter);
      for (const c of d.campos) campos.push(c);
    }
    return { campos: campos };
  }
  function avaliar(cand) {
    for (const [campo, valor] of cand.campos) {
      const t = termo(valor);
      if (t) return { campo: campo, termo: t };
    }
    return null;
  }
  function barrar(ev, b) {
    try { ev.preventDefault(); ev.stopImmediatePropagation(); ev.stopPropagation(); } catch (e) {}
    window.__sbfBloqueios = (window.__sbfBloqueios || []).concat([b]);
    try { if (window.sbfBloqueio) window.sbfBloqueio(JSON.stringify(b)); } catch (e) {}
  }
  function aoClicar(ev) {
    const alvo = ev.target && ev.target.nodeType === 1 ? ev.target : (ev.target && ev.target.parentElement);
    if (!alvo) return;
    const b = avaliar(descrever(alvo));
    if (b) barrar(ev, b);
  }
  function aoEnviar(ev) {
    const form = ev.target;
    if (!form || form.tagName !== 'FORM') return;
    const b = avaliar(formularioPorSubmit(form, ev.submitter));
    if (b) barrar(ev, b);
  }
  window.addEventListener('click', aoClicar, true);
  window.addEventListener('auxclick', aoClicar, true);
  window.addEventListener('submit', aoEnviar, true);
  // submit() e requestSubmit() programáticos não disparam o evento `submit`.
  try {
    const sub = HTMLFormElement.prototype.submit;
    HTMLFormElement.prototype.submit = function () {
      const b = avaliar(formularioPorSubmit(this, null));
      if (b) { barrar({ preventDefault() {}, stopImmediatePropagation() {}, stopPropagation() {} }, b); return; }
      return sub.apply(this, arguments);
    };
    const req = HTMLFormElement.prototype.requestSubmit;
    HTMLFormElement.prototype.requestSubmit = function (s) {
      const b = avaliar(formularioPorSubmit(this, s || null));
      if (b) { barrar({ preventDefault() {}, stopImmediatePropagation() {}, stopPropagation() {} }, b); return; }
      return req.apply(this, arguments);
    };
  } catch (e) {}

  function visivel(el) {
    const r = el.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) return false;
    const s = getComputedStyle(el);
    return s.visibility !== 'hidden' && s.display !== 'none';
  }
  function seletorDe(el) {
    if (el.id) {
      const s = '#' + CSS.escape(el.id);
      if (document.querySelectorAll(s).length === 1) return s;
    }
    const nome = el.getAttribute && el.getAttribute('name');
    if (nome) {
      const s = el.tagName.toLowerCase() + '[name="' + nome.replace(/"/g, '\\"') + '"]';
      if (document.querySelectorAll(s).length === 1) return s;
    }
    const partes = [];
    let n = el;
    while (n && n.nodeType === 1 && n !== document.documentElement) {
      if (n.id) { partes.unshift('#' + CSS.escape(n.id)); break; }
      let i = 1, p = n;
      while ((p = p.previousElementSibling)) if (p.tagName === n.tagName) i++;
      partes.unshift(n.tagName.toLowerCase() + ':nth-of-type(' + i + ')');
      n = n.parentElement;
    }
    return partes.join(' > ');
  }
  function textoVisivel() {
    const saida = [];
    let total = 0;
    const w = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT);
    while (w.nextNode() && total < 6000) {
      const n = w.currentNode;
      const pai = n.parentElement;
      if (!pai || /^(SCRIPT|STYLE|NOSCRIPT|TEXTAREA|SELECT|OPTION)$/.test(pai.tagName)) continue;
      const t = n.nodeValue.replace(/\s+/g, ' ').trim();
      if (!t || !visivel(pai)) continue;
      saida.push(t); total += t.length + 1;
    }
    return saida.join(' ');
  }
  function rotuloDe(el) {
    if (el.id) {
      const l = document.querySelector('label[for="' + el.id.replace(/"/g, '\\"') + '"]');
      if (l) return l.innerText.trim().slice(0, 60);
    }
    const p = el.closest('label');
    if (p) return p.innerText.trim().slice(0, 60);
    return (el.getAttribute('aria-label') || el.getAttribute('placeholder') || '').slice(0, 60);
  }
  function pagina() {
    const elementos = [];
    document.querySelectorAll('a[href],button,input[type=submit],input[type=button],[role=button],[onclick]').forEach((el) => {
      if (elementos.length >= 80 || !visivel(el)) return;
      const d = {
        seletor: seletorDe(el), tag: el.tagName.toLowerCase(),
        texto: (el.innerText || el.getAttribute('value') || '').replace(/\s+/g, ' ').trim().slice(0, 80),
        rotulo: (el.getAttribute('aria-label') || '').slice(0, 80),
      };
      const h = el.getAttribute('href');
      if (h) d.destino = h.slice(0, 200);
      elementos.push(d);
    });
    const formularios = [];
    document.querySelectorAll('form').forEach((f) => {
      if (formularios.length >= 10) return;
      const campos = [];
      f.querySelectorAll('input,select,textarea').forEach((c) => {
        const tipo = (c.getAttribute('type') || c.tagName.toLowerCase()).toLowerCase();
        if (tipo === 'hidden' || tipo === 'submit' || tipo === 'button') return;
        campos.push({
          seletor: seletorDe(c), tipo: tipo, nome: c.getAttribute('name') || '', rotulo: rotuloDe(c),
          autocomplete: c.getAttribute('autocomplete') || '',
          preenchido: !!c.value,   // só o fato, nunca o valor
        });
      });
      formularios.push({ seletor: seletorDe(f), acao: f.getAttribute('action') || '', metodo: (f.method || 'get').toLowerCase(), campos: campos });
    });
    const tabelas = [];
    document.querySelectorAll('table').forEach((t) => {
      if (tabelas.length >= 5 || !visivel(t)) return;
      const cab = Array.from(t.querySelectorAll('th')).slice(0, 12).map((h) => h.innerText.trim().slice(0, 40));
      tabelas.push({ seletor: seletorDe(t), linhas: t.rows.length, cabecalho: cab });
    });
    const quadros = [];
    document.querySelectorAll('iframe[src],.g-recaptcha,.h-captcha,.cf-turnstile,[data-sitekey]').forEach((q) => {
      const s = (q.getAttribute('src') || q.className || 'captcha').toString().slice(0, 80);
      quadros.push(s);
    });
    return { url: location.href, titulo: document.title.slice(0, 120), texto: textoVisivel(), elementos: elementos, formularios: formularios, tabelas: tabelas, quadros: quadros };
  }

  window.__sbfPonte = {
    descrever: (sel) => { const el = document.querySelector(sel); return el ? descrever(el) : null; },
    pagina: pagina,
    visivel: (sel) => { const el = document.querySelector(sel); return !!el && visivel(el); },
    bloqueios: () => window.__sbfBloqueios || [],
  };
})();
