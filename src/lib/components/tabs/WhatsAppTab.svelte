<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { addToast } from '$lib/stores/notifications';

  const BRIDGE = 'http://localhost:3142';

  // ── State ────────────────────────────────────────────────────────────────
  let connected    = $state(false);
  let bridgeAlive  = $state(false);   // bridge HTTP reachable but maybe not WA-connected
  let qrCode       = $state<string | null>(null);
  let metrics      = $state<any>(null);
  let contacts  = $state<{id: string; name: string}[]>([]);
  let groups    = $state<{id: string; name: string; participants: number}[]>([]);
  let history   = $state<{direction: string; jid: string; body: string; ts: number}[]>([]);
  let rules     = $state<any[]>([]);
  let scheduled = $state<any[]>([]);
  let templates = $state<any[]>([]);

  let activeSection = $state<'chat' | 'contacts' | 'rules' | 'schedule' | 'templates'>('chat');
  let selectedJid   = $state<string | null>(null);
  let jidHistory    = $state<any[]>([]);
  let jidLoading    = $state(false);

  // send form
  let sendTo      = $state('');
  let sendMsg     = $state('');
  let sending     = $state(false);

  // new rule form
  let showRuleForm   = $state(false);
  let ruleKeyword    = $state('');
  let ruleResponse   = $state('');
  let ruleCaseSens   = $state(false);
  let ruleExactMatch = $state(false);

  // new template form
  let showTplForm = $state(false);
  let tplName     = $state('');
  let tplBody     = $state('');

  // schedule form
  let showSchedForm = $state(false);
  let schedTo       = $state('');
  let schedMsg      = $state('');
  let schedAt       = $state('');

  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let loading = $state(true);
  let error   = $state('');
  let messagesEl: HTMLDivElement | undefined = $state();

  // Auto-scroll to bottom when jidHistory changes
  $effect(() => {
    if (jidHistory.length && messagesEl) {
      // Use tick-like delay to let DOM render
      requestAnimationFrame(() => {
        if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight;
      });
    }
  });

  // ── Bridge fetch helper ───────────────────────────────────────────────────
  async function bfetch(path: string, opts?: RequestInit) {
    const res = await fetch(BRIDGE + path, opts);
    if (!res.ok) throw new Error(`${res.status} ${path}`);
    return res.json();
  }

  async function bpost(path: string, body: any) {
    return bfetch(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  }

  // ── Load / poll ───────────────────────────────────────────────────────────
  async function loadStatus() {
    try {
      const m = await bfetch('/metrics');
      bridgeAlive = true;
      connected   = m.connected ?? false;
      metrics     = m;
      // The bridge exposes qr via /qr when awaiting authentication
      if (!connected) {
        try {
          const q = await bfetch('/qr');
          qrCode = q.qr ?? null;
        } catch { qrCode = null; }
      } else {
        qrCode = null;
      }
    } catch { bridgeAlive = false; connected = false; qrCode = null; }
  }

  async function loadAll() {
    loading = true;
    error = '';
    try {
      await loadStatus();
      if (connected) {
        const [c, g, h, r, sc, t, m] = await Promise.allSettled([
          bfetch('/contacts'),
          bfetch('/groups'),
          bfetch('/history?limit=50'),
          bfetch('/rules'),
          bfetch('/scheduled'),
          bfetch('/templates'),
          bfetch('/metrics'),
        ]);
        if (c.status === 'fulfilled') contacts  = c.value;
        if (g.status === 'fulfilled') groups    = g.value;
        if (h.status === 'fulfilled') history   = h.value;
        if (r.status === 'fulfilled') rules     = r.value;
        if (sc.status === 'fulfilled') scheduled = sc.value;
        if (t.status === 'fulfilled') templates = t.value;
        if (m.status === 'fulfilled') metrics   = m.value;
      }
    } catch (e) {
      error = 'No se puede conectar al wa-bridge (:3142). ¿Está corriendo?';
    } finally {
      loading = false;
    }
  }

  async function loadJidHistory(jid: string) {
    selectedJid = jid;
    sendTo = jid;
    jidLoading = true;
    try {
      jidHistory = await bfetch(`/history/${encodeURIComponent(jid)}?limit=30`);
    } catch { jidHistory = []; }
    jidLoading = false;
  }

  // Silent background refresh of history + selected chat (no loading spinner)
  async function pollMessages() {
    if (!connected) return;
    try {
      const h = await bfetch('/history?limit=50');
      history = h;
      // Also refresh the selected conversation if one is open
      if (selectedJid) {
        const jh = await bfetch(`/history/${encodeURIComponent(selectedJid)}?limit=30`);
        jidHistory = jh;
      }
    } catch { /* bridge may have gone down — loadStatus will catch it */ }
  }

  // Silent refresh of contacts/groups/rules/scheduled/templates (infrequent data)
  async function pollMetadata() {
    if (!connected) return;
    try {
      const [c, g, r, sc, t] = await Promise.allSettled([
        bfetch('/contacts'), bfetch('/groups'),
        bfetch('/rules'), bfetch('/scheduled'), bfetch('/templates'),
      ]);
      if (c.status === 'fulfilled') contacts = c.value;
      if (g.status === 'fulfilled') groups = g.value;
      if (r.status === 'fulfilled') rules = r.value;
      if (sc.status === 'fulfilled') scheduled = sc.value;
      if (t.status === 'fulfilled') templates = t.value;
    } catch {}
  }

  let metadataTimer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    loadAll();
    // Fast poll: status + messages every 5s
    pollTimer = setInterval(async () => {
      await loadStatus();
      await pollMessages();
    }, 5000);
    // Slow poll: contacts/groups/rules/templates every 30s
    metadataTimer = setInterval(pollMetadata, 30_000);
  });
  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (metadataTimer) clearInterval(metadataTimer);
  });

  // ── Actions ───────────────────────────────────────────────────────────────
  async function sendMessage() {
    if (!sendTo.trim() || !sendMsg.trim()) return;
    sending = true;
    try {
      await bpost('/send', { to: sendTo.trim(), message: sendMsg.trim() });
      addToast(`Enviado a ${sendTo}`, 'success');
      sendMsg = '';
      // Refresh messages silently (no loading spinner)
      await pollMessages();
    } catch (e: any) { addToast(e?.message ?? String(e), 'error'); }
    sending = false;
  }

  async function addRule() {
    if (!ruleKeyword.trim() || !ruleResponse.trim()) return;
    try {
      await bpost('/rules', { keyword: ruleKeyword, response: ruleResponse, case_sensitive: ruleCaseSens, exact_match: ruleExactMatch });
      addToast('Regla creada', 'success');
      ruleKeyword = ''; ruleResponse = ''; ruleCaseSens = false; ruleExactMatch = false;
      showRuleForm = false;
      rules = await bfetch('/rules');
    } catch (e: any) { addToast(e?.message ?? String(e), 'error'); }
  }

  async function deleteRule(id: string) {
    try {
      await fetch(`${BRIDGE}/rules/${id}`, { method: 'DELETE' });
      rules = rules.filter(r => r.id !== id);
    } catch (e: any) { addToast(e?.message ?? String(e), 'error'); }
  }

  async function addTemplate() {
    if (!tplName.trim() || !tplBody.trim()) return;
    try {
      await bpost('/templates', { name: tplName, body: tplBody });
      addToast('Plantilla creada', 'success');
      tplName = ''; tplBody = '';
      showTplForm = false;
      templates = await bfetch('/templates');
    } catch (e: any) { addToast(e?.message ?? String(e), 'error'); }
  }

  async function deleteTemplate(id: string) {
    try {
      await fetch(`${BRIDGE}/templates/${id}`, { method: 'DELETE' });
      templates = templates.filter(t => t.id !== id);
    } catch (e: any) { addToast(e?.message ?? String(e), 'error'); }
  }

  async function sendTemplate(id: string, to: string) {
    if (!to.trim()) { addToast('Especificá el destinatario', 'error'); return; }
    try {
      await bpost(`/templates/${id}/send`, { to });
      addToast('Plantilla enviada', 'success');
    } catch (e: any) { addToast(e?.message ?? String(e), 'error'); }
  }

  async function scheduleMessage() {
    if (!schedTo.trim() || !schedMsg.trim() || !schedAt) return;
    try {
      await bpost('/schedule', { to: schedTo, message: schedMsg, send_at: new Date(schedAt).toISOString() });
      addToast('Mensaje programado', 'success');
      schedTo = ''; schedMsg = ''; schedAt = '';
      showSchedForm = false;
      scheduled = await bfetch('/scheduled');
    } catch (e: any) { addToast(e?.message ?? String(e), 'error'); }
  }

  async function cancelScheduled(id: string) {
    try {
      await fetch(`${BRIDGE}/scheduled/${id}`, { method: 'DELETE' });
      scheduled = scheduled.filter(s => s.id !== id);
    } catch (e: any) { addToast(e?.message ?? String(e), 'error'); }
  }

  // ── Helpers ───────────────────────────────────────────────────────────────
  function timeAgo(ts: number) {
    const diff = Date.now() - ts;
    if (diff < 60_000) return 'ahora';
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h`;
    return `${Math.floor(diff / 86_400_000)}d`;
  }

  function displayName(jid: string) {
    const c = contacts.find(c => c.id === jid) || groups.find(g => g.id === jid);
    return c?.name ?? jid.split('@')[0];
  }

  // unique JIDs in history for contact list
  let chatList = $derived.by(() => {
    const map = new Map<string, {jid: string; lastMsg: string; lastTs: number; direction: string}>();
    for (const m of history) {
      const prev = map.get(m.jid);
      if (!prev || m.ts > prev.lastTs) map.set(m.jid, { jid: m.jid, lastMsg: m.body, lastTs: m.ts, direction: m.direction });
    }
    return [...map.values()].sort((a, b) => b.lastTs - a.lastTs);
  });
</script>

<div class="wa-tab">
  <!-- Header bar -->
  <div class="wa-header">
    <div class="wa-status" class:connected>
      <span class="wa-dot"></span>
      <span class="wa-status-text">{connected ? 'Conectado' : 'Desconectado'}</span>
      {#if metrics && connected}
        <span class="wa-metric">↑{metrics.messages_sent ?? 0} ↓{metrics.messages_received ?? 0}</span>
        <span class="wa-metric" title="Uptime">{Math.floor((metrics.uptime_seconds ?? 0) / 60)}m</span>
      {/if}
    </div>

    <nav class="wa-nav">
      {#each (['chat','contacts','rules','schedule','templates'] as const) as s}
        <button class="wa-nav-btn" class:active={activeSection === s} onclick={() => activeSection = s}>
          {s === 'chat' ? '💬 Chat' : s === 'contacts' ? '👥 Contactos' : s === 'rules' ? '🤖 Reglas' : s === 'schedule' ? '⏰ Programados' : '📋 Plantillas'}
        </button>
      {/each}
    </nav>

    <button class="wa-refresh-btn" onclick={loadAll} title="Actualizar">↻</button>
  </div>

  {#if loading}
    <div class="wa-loading">Conectando al bridge…</div>
  {:else if error}
    <div class="wa-error">
      <span>⚠ {error}</span>
      <button onclick={loadAll}>Reintentar</button>
    </div>
  {:else if !connected && qrCode}
    <!-- QR code from bridge /qr endpoint -->
    <div class="wa-qr-wrap">
      <p>Escaneá el QR con WhatsApp</p>
      <img src={qrCode} alt="QR WhatsApp" class="wa-qr" />
    </div>
  {:else if !connected && bridgeAlive}
    <!-- Bridge running but WhatsApp not authenticated yet -->
    <div class="wa-disconnected">
      <div class="wa-disc-icon">📲</div>
      <p>WhatsApp necesita autenticación.</p>
      <p class="wa-disc-hint">Revisá la terminal del wa-bridge para escanear el QR.</p>
      <code>cd wa-bridge && npm start</code>
      <button class="wa-retry-btn" onclick={loadAll}>↻ Verificar conexión</button>
    </div>
  {:else if !connected}
    <div class="wa-disconnected">
      <div class="wa-disc-icon">📵</div>
      <p>Bridge desconectado. Iniciá el wa-bridge:</p>
      <code>cd wa-bridge && npm start</code>
    </div>
  {:else}

    <!-- ── CHAT SECTION ── -->
    {#if activeSection === 'chat'}
      <div class="wa-chat-layout">
        <!-- Chat list (left) -->
        <div class="wa-chat-list">
          <div class="wa-section-title">Conversaciones</div>
          {#each chatList as chat}
            <button
              class="wa-chat-item"
              class:active={selectedJid === chat.jid}
              onclick={() => loadJidHistory(chat.jid)}
            >
              <div class="wa-avatar">{displayName(chat.jid).charAt(0).toUpperCase()}</div>
              <div class="wa-chat-info">
                <div class="wa-chat-name">{displayName(chat.jid)}</div>
                <div class="wa-chat-preview">{chat.direction === 'out' ? '→ ' : ''}{chat.lastMsg.slice(0, 40)}</div>
              </div>
              <div class="wa-chat-ts">{timeAgo(chat.lastTs)}</div>
            </button>
          {/each}
          {#if chatList.length === 0}
            <div class="wa-empty">Sin mensajes aún</div>
          {/if}
        </div>

        <!-- Message view (right) -->
        <div class="wa-message-view">
          {#if selectedJid}
            <div class="wa-msg-header">
              <strong>{displayName(selectedJid)}</strong>
              <span class="wa-jid-text">{selectedJid}</span>
            </div>
            <div class="wa-messages" bind:this={messagesEl}>
              {#if jidLoading}
                <div class="wa-loading-sm">Cargando…</div>
              {:else}
                {#each jidHistory as m}
                  <div class="wa-msg" class:outgoing={m.direction === 'out'}>
                    <div class="wa-msg-bubble">{m.body}</div>
                    <div class="wa-msg-ts">{timeAgo(m.ts)}</div>
                  </div>
                {/each}
              {/if}
            </div>
            <!-- Reply form -->
            <div class="wa-reply-form">
              <input bind:value={sendMsg} placeholder="Escribí un mensaje…" onkeydown={(e) => e.key === 'Enter' && !e.shiftKey && sendMessage()} />
              <button onclick={sendMessage} disabled={sending || !sendMsg.trim()}>Enviar</button>
            </div>
          {:else}
            <div class="wa-no-chat">
              <p>Seleccioná una conversación o enviá un mensaje nuevo</p>
              <!-- Send new message form -->
              <div class="wa-new-msg-form">
                <input bind:value={sendTo} placeholder="JID o nombre de grupo…" />
                <textarea bind:value={sendMsg} placeholder="Mensaje…" rows="3"></textarea>
                <button onclick={sendMessage} disabled={sending || !sendTo.trim() || !sendMsg.trim()}>
                  {sending ? 'Enviando…' : '📤 Enviar'}
                </button>
              </div>
            </div>
          {/if}
        </div>
      </div>

    <!-- ── CONTACTS SECTION ── -->
    {:else if activeSection === 'contacts'}
      <div class="wa-section">
        <div class="wa-section-title">Contactos ({contacts.length})</div>
        <div class="wa-contact-grid">
          {#each contacts as c}
            <button class="wa-contact-card" onclick={() => { activeSection = 'chat'; loadJidHistory(c.id); }}>
              <div class="wa-avatar">{c.name?.charAt(0)?.toUpperCase() ?? '?'}</div>
              <div class="wa-contact-info">
                <div class="wa-contact-name">{c.name ?? c.id}</div>
                <div class="wa-contact-jid">{c.id.split('@')[0]}</div>
              </div>
            </button>
          {/each}
        </div>
        {#if groups.length > 0}
          <div class="wa-section-title" style="margin-top:1rem">Grupos ({groups.length})</div>
          <div class="wa-contact-grid">
            {#each groups as g}
              <button class="wa-contact-card" onclick={() => { activeSection = 'chat'; loadJidHistory(g.id); }}>
                <div class="wa-avatar wa-avatar-group">👥</div>
                <div class="wa-contact-info">
                  <div class="wa-contact-name">{g.name}</div>
                  <div class="wa-contact-jid">{g.participants} miembros</div>
                </div>
              </button>
            {/each}
          </div>
        {/if}
      </div>

    <!-- ── RULES SECTION ── -->
    {:else if activeSection === 'rules'}
      <div class="wa-section">
        <div class="wa-section-header">
          <div class="wa-section-title">Auto-respuestas ({rules.length})</div>
          <button class="wa-add-btn" onclick={() => showRuleForm = !showRuleForm}>+ Nueva regla</button>
        </div>

        {#if showRuleForm}
          <div class="wa-form-card">
            <input bind:value={ruleKeyword} placeholder="Palabra clave (ej: hola)" />
            <textarea bind:value={ruleResponse} placeholder="Respuesta automática…" rows="2"></textarea>
            <div class="wa-form-row">
              <label><input type="checkbox" bind:checked={ruleCaseSens} /> Mayúsculas importan</label>
              <label><input type="checkbox" bind:checked={ruleExactMatch} /> Coincidencia exacta</label>
            </div>
            <div class="wa-form-actions">
              <button onclick={addRule}>Guardar</button>
              <button class="wa-btn-secondary" onclick={() => showRuleForm = false}>Cancelar</button>
            </div>
          </div>
        {/if}

        {#each rules as rule}
          <div class="wa-rule-card">
            <div class="wa-rule-info">
              <span class="wa-keyword">"{rule.keyword}"</span>
              <span class="wa-arrow">→</span>
              <span class="wa-rule-response">{rule.response}</span>
            </div>
            <div class="wa-rule-flags">
              {#if rule.case_sensitive}<span class="wa-flag">Aa</span>{/if}
              {#if rule.exact_match}<span class="wa-flag">=</span>{/if}
            </div>
            <button class="wa-delete-btn" onclick={() => deleteRule(rule.id)}>✕</button>
          </div>
        {/each}
        {#if rules.length === 0 && !showRuleForm}
          <div class="wa-empty">Sin reglas. Las reglas responden automáticamente cuando recibes ciertos mensajes.</div>
        {/if}
      </div>

    <!-- ── SCHEDULE SECTION ── -->
    {:else if activeSection === 'schedule'}
      <div class="wa-section">
        <div class="wa-section-header">
          <div class="wa-section-title">Mensajes programados ({scheduled.length})</div>
          <button class="wa-add-btn" onclick={() => showSchedForm = !showSchedForm}>+ Programar</button>
        </div>

        {#if showSchedForm}
          <div class="wa-form-card">
            <input bind:value={schedTo} placeholder="JID o nombre de grupo…" />
            <textarea bind:value={schedMsg} placeholder="Mensaje…" rows="2"></textarea>
            <input type="datetime-local" bind:value={schedAt} />
            <div class="wa-form-actions">
              <button onclick={scheduleMessage}>Programar</button>
              <button class="wa-btn-secondary" onclick={() => showSchedForm = false}>Cancelar</button>
            </div>
          </div>
        {/if}

        {#each scheduled as s}
          <div class="wa-sched-card">
            <div class="wa-sched-info">
              <span class="wa-sched-to">{displayName(s.to)}</span>
              <span class="wa-sched-msg">{s.message.slice(0, 60)}{s.message.length > 60 ? '…' : ''}</span>
              <span class="wa-sched-at">⏰ {new Date(s.send_at).toLocaleString()}</span>
            </div>
            <button class="wa-delete-btn" onclick={() => cancelScheduled(s.id)}>✕</button>
          </div>
        {/each}
        {#if scheduled.length === 0 && !showSchedForm}
          <div class="wa-empty">Sin mensajes programados.</div>
        {/if}
      </div>

    <!-- ── TEMPLATES SECTION ── -->
    {:else if activeSection === 'templates'}
      <div class="wa-section">
        <div class="wa-section-header">
          <div class="wa-section-title">Plantillas ({templates.length})</div>
          <button class="wa-add-btn" onclick={() => showTplForm = !showTplForm}>+ Nueva plantilla</button>
        </div>

        {#if showTplForm}
          <div class="wa-form-card">
            <input bind:value={tplName} placeholder="Nombre de la plantilla" />
            <textarea bind:value={tplBody} placeholder={"Cuerpo. Usá {{name}}, {{date}}, {{time}}…"} rows="3"></textarea>
            <div class="wa-form-actions">
              <button onclick={addTemplate}>Guardar</button>
              <button class="wa-btn-secondary" onclick={() => showTplForm = false}>Cancelar</button>
            </div>
          </div>
        {/if}

        {#each templates as tpl}
          <div class="wa-tpl-card">
            <div class="wa-tpl-info">
              <strong>{tpl.name}</strong>
              <p class="wa-tpl-body">{tpl.body}</p>
            </div>
            <div class="wa-tpl-actions">
              <input
                class="wa-tpl-to"
                placeholder="Enviar a…"
                onkeydown={(e) => { if (e.key === 'Enter') { sendTemplate(tpl.id, (e.target as HTMLInputElement).value); (e.target as HTMLInputElement).value = ''; } }}
              />
              <button class="wa-delete-btn" onclick={() => deleteTemplate(tpl.id)}>✕</button>
            </div>
          </div>
        {/each}
        {#if templates.length === 0 && !showTplForm}
          <div class="wa-empty">Sin plantillas. Creá una para enviar mensajes reutilizables con variables.</div>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .wa-tab { display: flex; flex-direction: column; height: 100%; background: var(--bg-1); overflow: hidden; }

  /* Header */
  .wa-header { display: flex; align-items: center; gap: 0.75rem; padding: 0.5rem 1rem; background: var(--bg-2); border-bottom: 1px solid var(--border); flex-shrink: 0; flex-wrap: wrap; }
  .wa-status { display: flex; align-items: center; gap: 0.4rem; font-size: 0.75rem; }
  .wa-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-3); }
  .wa-status.connected .wa-dot { background: #22c55e; box-shadow: 0 0 6px #22c55e88; }
  .wa-status-text { color: var(--text-2); }
  .wa-metric { color: var(--text-3); font-size: 0.7rem; padding: 0.1rem 0.4rem; background: var(--bg-3); border-radius: 4px; }
  .wa-nav { display: flex; gap: 0.25rem; flex: 1; }
  .wa-nav-btn { padding: 0.25rem 0.6rem; border: 1px solid var(--border); border-radius: 6px; background: transparent; color: var(--text-2); cursor: pointer; font-size: 0.72rem; transition: all 0.15s; }
  .wa-nav-btn:hover { background: var(--bg-3); color: var(--text-1); }
  .wa-nav-btn.active { background: var(--accent, #06b6d4); color: #000; border-color: var(--accent, #06b6d4); }
  .wa-refresh-btn { background: transparent; border: 1px solid var(--border); border-radius: 6px; color: var(--text-2); cursor: pointer; padding: 0.25rem 0.5rem; font-size: 0.85rem; }
  .wa-refresh-btn:hover { color: var(--text-1); }

  /* States */
  .wa-loading, .wa-loading-sm { padding: 2rem; text-align: center; color: var(--text-3); font-size: 0.85rem; }
  .wa-loading-sm { padding: 1rem; }
  .wa-error { padding: 1rem; display: flex; gap: 1rem; align-items: center; color: #f87171; }
  .wa-error button { padding: 0.25rem 0.75rem; border: 1px solid #f87171; border-radius: 6px; background: transparent; color: #f87171; cursor: pointer; }
  .wa-empty { padding: 1.5rem; text-align: center; color: var(--text-3); font-size: 0.82rem; }

  /* QR */
  .wa-qr-wrap { display: flex; flex-direction: column; align-items: center; padding: 2rem; gap: 1rem; }
  .wa-qr-wrap p { color: var(--text-2); }
  .wa-qr { width: 200px; height: 200px; border-radius: 8px; }

  /* Disconnected */
  .wa-disconnected { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; gap: 0.75rem; color: var(--text-2); }
  .wa-disc-icon { font-size: 3rem; }
  .wa-disconnected code { background: var(--bg-3); padding: 0.5rem 1rem; border-radius: 6px; font-size: 0.82rem; color: var(--text-1); }
  .wa-disc-hint { font-size: 0.85rem; opacity: 0.8; }
  .wa-retry-btn { margin-top: 0.5rem; padding: 0.4rem 1rem; border-radius: 6px; border: 1px solid var(--border); background: var(--bg-2); color: var(--text-1); cursor: pointer; font-size: 0.85rem; }
  .wa-retry-btn:hover { background: var(--bg-3); }

  /* Chat layout */
  .wa-chat-layout { display: flex; flex: 1; overflow: hidden; }
  .wa-chat-list { width: 220px; flex-shrink: 0; border-right: 1px solid var(--border); overflow-y: auto; }
  .wa-chat-item { display: flex; align-items: center; gap: 0.5rem; padding: 0.6rem 0.75rem; width: 100%; text-align: left; background: transparent; border: none; cursor: pointer; border-bottom: 1px solid var(--border); transition: background 0.1s; }
  .wa-chat-item:hover { background: var(--bg-3); }
  .wa-chat-item.active { background: var(--bg-3); border-left: 2px solid var(--accent, #06b6d4); }
  .wa-chat-info { flex: 1; min-width: 0; }
  .wa-chat-name { font-size: 0.8rem; font-weight: 600; color: var(--text-1); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .wa-chat-preview { font-size: 0.7rem; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .wa-chat-ts { font-size: 0.65rem; color: var(--text-3); white-space: nowrap; }

  /* Avatar */
  .wa-avatar { width: 32px; height: 32px; border-radius: 50%; background: var(--bg-4, #334155); display: flex; align-items: center; justify-content: center; font-size: 0.85rem; font-weight: 700; color: var(--text-1); flex-shrink: 0; }
  .wa-avatar-group { font-size: 0.75rem; }

  /* Message view */
  .wa-message-view { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  .wa-msg-header { padding: 0.6rem 1rem; border-bottom: 1px solid var(--border); background: var(--bg-2); }
  .wa-msg-header strong { font-size: 0.85rem; color: var(--text-1); }
  .wa-jid-text { font-size: 0.7rem; color: var(--text-3); margin-left: 0.5rem; }
  .wa-messages { flex: 1; overflow-y: auto; padding: 0.75rem 1rem; display: flex; flex-direction: column; gap: 0.4rem; }
  .wa-msg { display: flex; flex-direction: column; max-width: 70%; }
  .wa-msg.outgoing { align-self: flex-end; align-items: flex-end; }
  .wa-msg-bubble { padding: 0.4rem 0.7rem; border-radius: 12px; background: var(--bg-3); font-size: 0.82rem; color: var(--text-1); word-break: break-word; }
  .wa-msg.outgoing .wa-msg-bubble { background: var(--accent, #06b6d4); color: #000; }
  .wa-msg-ts { font-size: 0.65rem; color: var(--text-3); margin-top: 0.15rem; }
  .wa-reply-form { display: flex; gap: 0.5rem; padding: 0.6rem 1rem; border-top: 1px solid var(--border); }
  .wa-reply-form input { flex: 1; padding: 0.4rem 0.75rem; background: var(--bg-3); border: 1px solid var(--border); border-radius: 20px; color: var(--text-1); font-size: 0.82rem; }
  .wa-reply-form button { padding: 0.4rem 1rem; background: var(--accent, #06b6d4); color: #000; border: none; border-radius: 20px; cursor: pointer; font-size: 0.82rem; font-weight: 600; }
  .wa-reply-form button:disabled { opacity: 0.5; cursor: not-allowed; }

  /* No chat / new message */
  .wa-no-chat { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; gap: 1rem; padding: 2rem; }
  .wa-no-chat p { color: var(--text-2); font-size: 0.85rem; }
  .wa-new-msg-form { display: flex; flex-direction: column; gap: 0.5rem; width: 100%; max-width: 400px; }
  .wa-new-msg-form input, .wa-new-msg-form textarea { padding: 0.5rem 0.75rem; background: var(--bg-3); border: 1px solid var(--border); border-radius: 8px; color: var(--text-1); font-size: 0.82rem; }
  .wa-new-msg-form button { padding: 0.5rem; background: var(--accent, #06b6d4); color: #000; border: none; border-radius: 8px; cursor: pointer; font-weight: 600; }
  .wa-new-msg-form button:disabled { opacity: 0.5; }

  /* Section generic */
  .wa-section { flex: 1; overflow-y: auto; padding: 1rem; display: flex; flex-direction: column; gap: 0.5rem; }
  .wa-section-title { font-size: 0.72rem; font-weight: 600; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.08em; padding-bottom: 0.25rem; }
  .wa-section-header { display: flex; justify-content: space-between; align-items: center; }
  .wa-add-btn { padding: 0.25rem 0.75rem; background: var(--accent, #06b6d4); color: #000; border: none; border-radius: 6px; cursor: pointer; font-size: 0.78rem; font-weight: 600; }

  /* Contacts */
  .wa-contact-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 0.5rem; }
  .wa-contact-card { display: flex; gap: 0.6rem; align-items: center; padding: 0.6rem; background: var(--bg-2); border: 1px solid var(--border); border-radius: 8px; cursor: pointer; text-align: left; transition: background 0.1s; }
  .wa-contact-card:hover { background: var(--bg-3); }
  .wa-contact-name { font-size: 0.8rem; font-weight: 600; color: var(--text-1); }
  .wa-contact-jid { font-size: 0.68rem; color: var(--text-3); }

  /* Form card */
  .wa-form-card { background: var(--bg-2); border: 1px solid var(--border); border-radius: 8px; padding: 0.75rem; display: flex; flex-direction: column; gap: 0.5rem; margin-bottom: 0.25rem; }
  .wa-form-card input, .wa-form-card textarea { padding: 0.4rem 0.6rem; background: var(--bg-3); border: 1px solid var(--border); border-radius: 6px; color: var(--text-1); font-size: 0.82rem; }
  .wa-form-row { display: flex; gap: 1rem; font-size: 0.78rem; color: var(--text-2); }
  .wa-form-row label { display: flex; align-items: center; gap: 0.3rem; cursor: pointer; }
  .wa-form-actions { display: flex; gap: 0.5rem; }
  .wa-form-actions button { padding: 0.3rem 0.75rem; background: var(--accent, #06b6d4); color: #000; border: none; border-radius: 6px; cursor: pointer; font-size: 0.8rem; font-weight: 600; }
  .wa-btn-secondary { background: var(--bg-3) !important; color: var(--text-2) !important; border: 1px solid var(--border) !important; }

  /* Rules */
  .wa-rule-card { display: flex; align-items: center; gap: 0.75rem; background: var(--bg-2); border: 1px solid var(--border); border-radius: 8px; padding: 0.6rem 0.75rem; }
  .wa-rule-info { flex: 1; display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .wa-keyword { font-size: 0.8rem; color: #f59e0b; font-family: monospace; }
  .wa-arrow { color: var(--text-3); }
  .wa-rule-response { font-size: 0.8rem; color: var(--text-2); }
  .wa-rule-flags { display: flex; gap: 0.25rem; }
  .wa-flag { font-size: 0.65rem; padding: 0.1rem 0.3rem; background: var(--bg-3); border: 1px solid var(--border); border-radius: 4px; color: var(--text-3); }
  .wa-delete-btn { background: transparent; border: none; color: var(--text-3); cursor: pointer; font-size: 0.8rem; padding: 0.2rem 0.4rem; border-radius: 4px; }
  .wa-delete-btn:hover { color: #f87171; background: var(--bg-3); }

  /* Schedule */
  .wa-sched-card { display: flex; align-items: flex-start; gap: 0.75rem; background: var(--bg-2); border: 1px solid var(--border); border-radius: 8px; padding: 0.6rem 0.75rem; }
  .wa-sched-info { flex: 1; display: flex; flex-direction: column; gap: 0.15rem; }
  .wa-sched-to { font-size: 0.8rem; font-weight: 600; color: var(--text-1); }
  .wa-sched-msg { font-size: 0.78rem; color: var(--text-2); }
  .wa-sched-at { font-size: 0.7rem; color: #f59e0b; }

  /* Templates */
  .wa-tpl-card { background: var(--bg-2); border: 1px solid var(--border); border-radius: 8px; padding: 0.7rem 0.75rem; display: flex; gap: 0.75rem; align-items: flex-start; }
  .wa-tpl-info { flex: 1; }
  .wa-tpl-info strong { font-size: 0.82rem; color: var(--text-1); }
  .wa-tpl-body { font-size: 0.75rem; color: var(--text-3); margin: 0.25rem 0 0; white-space: pre-wrap; }
  .wa-tpl-actions { display: flex; gap: 0.5rem; align-items: center; flex-shrink: 0; }
  .wa-tpl-to { padding: 0.3rem 0.5rem; background: var(--bg-3); border: 1px solid var(--border); border-radius: 6px; color: var(--text-1); font-size: 0.75rem; width: 130px; }
</style>
