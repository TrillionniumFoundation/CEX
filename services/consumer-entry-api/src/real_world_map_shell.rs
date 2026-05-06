use super::*;

pub(super) fn trillionnium_language_settings_html() -> &'static str {
    r#"<article id="trillionnium-system-language-settings" class="module system-settings" data-i18n-preserve="1">
  <strong data-i18n-en="System Settings" data-i18n-zh="系统设置">System Settings</strong>
  <span data-i18n-en="Interface language" data-i18n-zh="界面语言">Interface language</span>
  <p data-i18n-en="Choose one UI language. The product no longer shows English and Chinese at the same time; more languages can be added here later." data-i18n-zh="选择一种界面语言。产品不再同时显示中英文；以后新增语言也从这里扩展。">Choose one UI language. The product no longer shows English and Chinese at the same time; more languages can be added here later.</p>
  <label for="trillionnium-language-select" class="sr-only" data-i18n-en="Language" data-i18n-zh="语言">Language</label>
  <select id="trillionnium-language-select" data-trillionnium-language-select aria-label="Language" data-i18n-aria-label-en="Language" data-i18n-aria-label-zh="语言">
    <option value="en">English</option>
    <option value="zh" data-i18n-en="Chinese" data-i18n-zh="中文">Chinese</option>
    <option value="__future" disabled data-i18n-en="More languages coming" data-i18n-zh="更多语言即将加入">More languages coming</option>
  </select>
  <p class="subtitle" data-i18n-en="Saved locally on this device." data-i18n-zh="语言设置会保存在当前设备。">Saved locally on this device.</p>
</article>"#
}

pub(super) fn trillionnium_language_inline_switcher_html(id: &str) -> String {
    format!(
        r#"<label id="{id}-label" class="language-switcher" for="{id}">
  <span data-i18n-en="Language" data-i18n-zh="语言">Language</span>
  <select id="{id}" data-trillionnium-language-select aria-label="Language" data-i18n-aria-label-en="Language" data-i18n-aria-label-zh="语言">
    <option value="en">English</option>
    <option value="zh" data-i18n-en="Chinese" data-i18n-zh="中文">Chinese</option>
    <option value="__future" disabled data-i18n-en="More languages coming" data-i18n-zh="更多语言即将加入">More languages coming</option>
  </select>
</label>"#,
        id = id
    )
}

pub(super) fn trillionnium_language_runtime_script() -> &'static str {
    r#"<script>
(function () {
  const STORAGE_KEY = 'trillionnium.ui.language';
  const COOKIE_KEY = 'trillionnium_lang';
  const DEFAULT_LANGUAGE = 'en';
  const textOriginals = new WeakMap();
  const attrOriginals = new WeakMap();
  const controlOriginals = new WeakMap();
  let activeLanguage = DEFAULT_LANGUAGE;
  let observer = null;

  function hasCjk(value) { return /[\u3400-\u9fff\uf900-\ufaff]/.test(String(value || '')); }
  function hasLatin(value) { return /[A-Za-z]/.test(String(value || '')); }
  function clean(value) { return String(value || '').replace(/\s+/g, ' ').trim(); }
  function supportedLanguages() {
    const languages = new Set(['en', 'zh']);
    document.querySelectorAll('[data-trillionnium-language-select] option[value]').forEach((option) => {
      const value = String(option.value || '').toLowerCase();
      if (value && !value.startsWith('__')) languages.add(value);
    });
    return languages;
  }
  function supported(value) { return supportedLanguages().has(String(value || '').toLowerCase()); }
  function readCookieLanguage() {
    const match = document.cookie.match(new RegExp('(?:^|; )' + COOKIE_KEY + '=([^;]*)'));
    return match ? decodeURIComponent(match[1]) : '';
  }
  function requestedLanguage() {
    const params = new URLSearchParams(window.location.search || '');
    const fromQuery = (params.get('lang') || '').toLowerCase();
    if (supported(fromQuery)) {
      try { window.localStorage.setItem(STORAGE_KEY, fromQuery); } catch (_) {}
      return fromQuery;
    }
    try {
      const stored = (window.localStorage.getItem(STORAGE_KEY) || '').toLowerCase();
      if (supported(stored)) return stored;
    } catch (_) {}
    const cookieLang = readCookieLanguage().toLowerCase();
    if (supported(cookieLang)) return cookieLang;
    const browserLang = String(navigator.language || '').toLowerCase();
    if (browserLang.startsWith('zh')) return 'zh';
    return DEFAULT_LANGUAGE;
  }
  function persistLanguage(language) {
    const next = supported(language) ? language : DEFAULT_LANGUAGE;
    try { window.localStorage.setItem(STORAGE_KEY, next); } catch (_) {}
    document.cookie = COOKIE_KEY + '=' + encodeURIComponent(next) + '; Max-Age=31536000; Path=/; SameSite=Lax';
  }
  function choosePair(left, right, language) {
    const leftClean = clean(left);
    const rightClean = clean(right);
    if (!leftClean || !rightClean) return leftClean || rightClean;
    const leftCjk = hasCjk(leftClean);
    const rightCjk = hasCjk(rightClean);
    const leftLatin = hasLatin(leftClean);
    const rightLatin = hasLatin(rightClean);
    if (language === 'zh') {
      if (rightCjk) return rightClean;
      if (leftCjk) return leftClean;
      return rightClean || leftClean;
    }
    if (leftLatin && !leftCjk) return leftClean;
    if (rightLatin && !rightCjk) return rightClean;
    return leftClean || rightClean;
  }
  function stripForLanguage(piece, language) {
    const source = clean(piece);
    if (!source) return '';
    if (language === 'zh') {
      if (!hasCjk(source)) return '';
      const chars = Array.from(source);
      let first = -1;
      let last = -1;
      chars.forEach((char, index) => {
        if (hasCjk(char)) {
          if (first < 0) first = index;
          last = index;
        }
      });
      return first >= 0 ? chars.slice(first, last + 1).join('').trim() : '';
    }
    if (!hasLatin(source)) return '';
    return source
      .replace(/[\u3400-\u9fff\uf900-\ufaff]+[：:，,、;；。.!?？\s-]*/g, ' ')
      .replace(/^[：:，,、;；。.!?？\s-]+/, '')
      .replace(/[：:，,、;；。.!?？\s-]+$/, '')
      .replace(/\s{2,}/g, ' ')
      .trim();
  }
  function joinLocalizedPieces(pieces, language) {
    const seen = new Set();
    const chosen = [];
    pieces.forEach((piece) => {
      const next = stripForLanguage(piece, language);
      if (next && !seen.has(next)) {
        seen.add(next);
        chosen.push(next);
      }
    });
    if (!chosen.length) return '';
    if (chosen.length === 1) return chosen[0];
    return chosen.join(language === 'zh' ? '：' : ': ');
  }
  function localizeChunk(chunk, language) {
    if (!chunk) return chunk;
    if (chunk.indexOf(' / ') !== -1) {
      const pieces = chunk.split(' / ');
      if (pieces.length === 2) return choosePair(pieces[0], pieces[1], language);
      return joinLocalizedPieces(pieces, language) || chunk;
    }
    if (chunk.indexOf(' · ') !== -1) {
      const pieces = chunk.split(' · ');
      const joined = joinLocalizedPieces(pieces, language);
      if (joined) return joined;
    }
    return chunk;
  }
  function localizeText(value, language) {
    const input = String(value || '');
    if (input.indexOf(' / ') === -1 && input.indexOf(' · ') === -1) {
      if (hasCjk(input) && hasLatin(input)) return stripForLanguage(input, language) || input;
      return input;
    }
    const parts = input.split(/(\s+—\s+|[,，;；。.!?？]\s*)/g);
    const localized = parts.map((part) => localizeChunk(part, language)).join('');
    const normalized = localized.replace(/\s+([,，;；。.!?？])/g, '$1').replace(/\s{2,}/g, ' ').trim();
    if (hasCjk(normalized) && hasLatin(normalized)) return stripForLanguage(normalized, language) || normalized || input;
    return normalized || input;
  }
  function shouldSkipNode(node) {
    const parent = node && node.parentElement;
    if (!parent) return true;
    if (parent.closest('[data-i18n-preserve="1"]')) return true;
    return !!parent.closest('script,style,code,pre,noscript');
  }
  function applyDatasetElement(element, language) {
    const value = element.getAttribute('data-i18n-' + language)
      ?? element.getAttribute('data-i18n-en')
      ?? element.getAttribute('data-i18n-zh');
    if (value !== null) element.textContent = value;
  }
  function applyTextNode(node, language) {
    if (shouldSkipNode(node)) return;
    if (!textOriginals.has(node)) textOriginals.set(node, node.nodeValue || '');
    const original = textOriginals.get(node) || '';
    const next = localizeText(original, language);
    if (node.nodeValue !== next) node.nodeValue = next;
  }
  function attributeStore(element) {
    let store = attrOriginals.get(element);
    if (!store) { store = {}; attrOriginals.set(element, store); }
    return store;
  }
  function applyAttribute(element, attr, language) {
    const explicit = element.getAttribute('data-i18n-' + attr + '-' + language)
      ?? element.getAttribute('data-i18n-' + attr + '-en')
      ?? element.getAttribute('data-i18n-' + attr + '-zh');
    if (explicit !== null) {
      element.setAttribute(attr, explicit);
      return;
    }
    if (!element.hasAttribute(attr)) return;
    const store = attributeStore(element);
    if (!(attr in store)) store[attr] = element.getAttribute(attr) || '';
    element.setAttribute(attr, localizeText(store[attr], language));
  }
  function applyControlValue(element, language) {
    if (!/^(TEXTAREA|INPUT)$/.test(element.tagName || '')) return;
    const type = String(element.getAttribute('type') || '').toLowerCase();
    if (['hidden', 'password', 'file', 'checkbox', 'radio'].includes(type)) return;
    if (!controlOriginals.has(element)) controlOriginals.set(element, element.value || '');
    if (document.activeElement === element) return;
    const explicit = element.getAttribute('data-i18n-value-' + language)
      ?? element.getAttribute('data-i18n-value-en')
      ?? element.getAttribute('data-i18n-value-zh');
    const original = explicit !== null ? explicit : (controlOriginals.get(element) || '');
    const next = explicit !== null ? explicit : localizeText(original, language);
    if (next && element.value !== next) element.value = next;
  }
  function walk(root, language) {
    if (!root) return;
    if (root.nodeType === Node.TEXT_NODE) { applyTextNode(root, language); return; }
    if (root.nodeType !== Node.ELEMENT_NODE && root.nodeType !== Node.DOCUMENT_NODE) return;
    const element = root.nodeType === Node.ELEMENT_NODE ? root : null;
    if (element) {
      applyDatasetElement(element, language);
      applyAttribute(element, 'placeholder', language);
      applyAttribute(element, 'aria-label', language);
      applyAttribute(element, 'title', language);
      applyControlValue(element, language);
    }
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT);
    let node = walker.currentNode;
    while (node) {
      if (node.nodeType === Node.ELEMENT_NODE) {
        applyDatasetElement(node, language);
        applyAttribute(node, 'placeholder', language);
        applyAttribute(node, 'aria-label', language);
        applyAttribute(node, 'title', language);
        applyControlValue(node, language);
      } else if (node.nodeType === Node.TEXT_NODE) {
        applyTextNode(node, language);
      }
      node = walker.nextNode();
    }
  }
  function syncSelectors(language) {
    document.querySelectorAll('[data-trillionnium-language-select]').forEach((select) => {
      if (select.value !== language) select.value = language;
      if (!select.dataset.languageBound) {
        select.dataset.languageBound = '1';
        select.addEventListener('change', () => {
          if (supported(select.value)) window.TrillionniumLanguage.set(select.value);
        });
      }
    });
  }
  function applyLanguage(language) {
    const next = supported(language) ? language : DEFAULT_LANGUAGE;
    activeLanguage = next;
    document.documentElement.lang = next === 'zh' ? 'zh-CN' : next;
    document.documentElement.setAttribute('data-ui-language', next);
    if (observer) observer.disconnect();
    walk(document.body, next);
    syncSelectors(next);
    if (observer) observer.observe(document.body, { childList: true, subtree: true });
    window.dispatchEvent(new CustomEvent('trillionnium:languagechange', { detail: { language: next } }));
  }
  window.TrillionniumLanguage = {
    get: () => activeLanguage,
    set: (language) => { persistLanguage(language); applyLanguage(language); },
    apply: () => applyLanguage(activeLanguage),
  };
  document.addEventListener('DOMContentLoaded', () => {
    activeLanguage = requestedLanguage();
    persistLanguage(activeLanguage);
    observer = new MutationObserver((mutations) => {
      if (observer) observer.disconnect();
      for (const mutation of mutations) {
        mutation.addedNodes.forEach((node) => walk(node, activeLanguage));
      }
      syncSelectors(activeLanguage);
      if (observer) observer.observe(document.body, { childList: true, subtree: true });
    });
    applyLanguage(activeLanguage);
  });
})();
</script>"#
}

#[derive(Debug, Clone, Copy)]
pub(super) enum RealWorldMapShellCardStyle {
    AppModule,
    WorldMini,
}

pub(super) fn real_world_map_runtime_bootstrap_js() -> &'static str {
    r#"      const createRealWorldMapAdapter = () => ({
        adapterId: (((engine.renderer_adapter || {}).adapter_id) || 'leaflet_renderer_adapter_v1'),
        createMap(targetElement, engineConfig, centerPoint) {
          return L.map(targetElement, { zoomControl: true }).setView([centerPoint.lat, centerPoint.lng], engineConfig.zoom || 15);
        },
        setBaseLayer(map, engineConfig) {
          return L.tileLayer(engineConfig.tile_url_template || 'https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
            maxZoom: engineConfig.max_zoom || 19,
            attribution: engineConfig.attribution || '© OpenStreetMap contributors'
          }).addTo(map);
        },
        createOverlayLayer(map) {
          return L.layerGroup().addTo(map);
        },
        clearOverlay(layer) {
          if (layer && Array.isArray(layer._trillionniumAnimationCancels)) {
            layer._trillionniumAnimationCancels.forEach((cancel) => {
              try { cancel(); } catch (_) {}
            });
            layer._trillionniumAnimationCancels = [];
          }
          if (layer && layer.clearLayers) layer.clearLayers();
        },
        latLngBounds(points) {
          return L.latLngBounds(points);
        },
        renderRouteLine(map, fromPoint, toPoint, options = {}) {
          return L.polyline([[fromPoint.lat, fromPoint.lng], [toPoint.lat, toPoint.lng]], options).addTo(map);
        },
        renderPoiMarker(map, marker, popupHtml) {
          return L.marker([marker.lat, marker.lng], { title: marker.name || marker.node_id }).addTo(map).bindPopup(popupHtml);
        },
        renderDensityCircle(layer, centerPoint, radiusMeters, options = {}) {
          return L.circle([centerPoint.lat, centerPoint.lng], { radius: radiusMeters, ...options }).addTo(layer);
        },
        renderRegionAnchor(layer, centerPoint, options = {}) {
          return L.circleMarker([centerPoint.lat, centerPoint.lng], options).addTo(layer);
        },
        renderTileFrame(layer, bounds, options = {}) {
          return L.rectangle(bounds, options).addTo(layer);
        },
        renderEventPulse(layer, marker, options = {}) {
          return L.circleMarker([marker.lat, marker.lng], options).addTo(layer);
        },
        renderPlayerAvatar(layer, avatar, popupHtml) {
          const icon = L.divIcon({
            className: 'trillionnium-player-avatar-marker',
            html: `<div style="width:34px;height:34px;border-radius:999px;display:grid;place-items:center;background:linear-gradient(135deg,#64e3ff,#f8c35b);box-shadow:0 0 0 3px rgba(11,18,32,.82),0 10px 24px rgba(0,0,0,.42);font-size:20px;transform:translateY(-4px);">${escapeHtml(avatar.icon || '🧍')}</div>`,
            iconSize: [34, 34],
            iconAnchor: [17, 30],
          });
          return L.marker([avatar.lat, avatar.lng], { icon, title: avatar.display_name || avatar.matrix_user_id || 'Player avatar', zIndexOffset: 900 }).addTo(layer).bindPopup(popupHtml);
        },
        renderMovingAvatar(layer, runner, popupHtml) {
          const from = runner.from || {};
          const to = runner.to || {};
          const fromLat = Number(from.lat);
          const fromLng = Number(from.lng);
          const toLat = Number(to.lat);
          const toLng = Number(to.lng);
          if (!Number.isFinite(fromLat) || !Number.isFinite(fromLng) || !Number.isFinite(toLat) || !Number.isFinite(toLng)) return null;
          const ratio = Math.max(0, Math.min(1, Number(runner.progress_ratio ?? 0.2)));
          const current = runner.current || {};
          const currentLat = Number(current.lat);
          const currentLng = Number(current.lng);
          const startLat = Number.isFinite(currentLat) ? currentLat : fromLat + (toLat - fromLat) * ratio;
          const startLng = Number.isFinite(currentLng) ? currentLng : fromLng + (toLng - fromLng) * ratio;
          const icon = L.divIcon({
            className: 'trillionnium-avatar-route-runner',
            html: `<div class="trillionnium-avatar-route-runner-dot"><span>${escapeHtml(runner.runner_icon || '🏃')}</span></div>`,
            iconSize: [38, 38],
            iconAnchor: [19, 32],
          });
          const marker = L.marker([startLat, startLng], { icon, title: runner.movement_label || runner.task_id || 'Avatar runner', zIndexOffset: 1100 }).addTo(layer).bindPopup(popupHtml);
          const sameNode = Math.abs(fromLat - toLat) < 0.000001 && Math.abs(fromLng - toLng) < 0.000001;
          if (!sameNode && typeof requestAnimationFrame === 'function') {
            let alive = true;
            let frameId = 0;
            const duration = Math.max(1800, Number(runner.animation_duration_ms || 4800));
            const startedAt = performance.now();
            const animate = (now) => {
              if (!alive || !layer || !layer.hasLayer || !layer.hasLayer(marker)) return;
              const phase = (ratio + ((now - startedAt) % duration) / duration) % 1;
              const eased = phase;
              marker.setLatLng([fromLat + (toLat - fromLat) * eased, fromLng + (toLng - fromLng) * eased]);
              frameId = requestAnimationFrame(animate);
            };
            const cancel = () => {
              alive = false;
              if (frameId) cancelAnimationFrame(frameId);
            };
            layer._trillionniumAnimationCancels = layer._trillionniumAnimationCancels || [];
            layer._trillionniumAnimationCancels.push(cancel);
            frameId = requestAnimationFrame(animate);
          }
          return marker;
        },
        getCenter(map) {
          return map.getCenter();
        },
        getZoom(map) {
          return map.getZoom();
        },
        onViewportChange(map, callback) {
          map.on('moveend zoomend', callback);
        },
        setOverlayVisibility(map, layer, enabled) {
          if (!layer) return;
          const attached = map.hasLayer(layer);
          if (enabled && !attached) map.addLayer(layer);
          if (!enabled && attached) map.removeLayer(layer);
        },
        fitBounds(map, bounds) {
          if (bounds && bounds.isValid && bounds.isValid()) map.fitBounds(bounds, { padding: [22, 22] });
        },
        focus(map, focusPayload) {
          if (!focusPayload) return;
          const lat = Number(focusPayload.lat);
          const lng = Number(focusPayload.lng);
          const zoom = Number(focusPayload.zoom || map.getZoom());
          if (Number.isFinite(lat) && Number.isFinite(lng)) map.setView([lat, lng], Number.isFinite(zoom) ? zoom : map.getZoom());
        },
        invalidateSize(map) {
          map.invalidateSize();
        },
      });
      const mapAdapter = createRealWorldMapAdapter();
      const mapRuntime = mapAdapter.createMap(target, engine, center);
      mapAdapter.setBaseLayer(mapRuntime, engine);
      const escapeHtml = (value) => String(value ?? '').replace(/[&<>"']/g, (ch) => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
      }[ch]));
      const mapLanguage = () => {
        const fromRuntime = window.TrillionniumLanguage && typeof window.TrillionniumLanguage.get === 'function'
          ? String(window.TrillionniumLanguage.get() || '').toLowerCase()
          : '';
        if (fromRuntime) return fromRuntime;
        const fromDocument = String(document.documentElement.getAttribute('data-ui-language') || '').toLowerCase();
        if (fromDocument) return fromDocument;
        const fromQuery = String(new URLSearchParams(window.location.search || '').get('lang') || '').toLowerCase();
        if (fromQuery) return fromQuery;
        return 'en';
      };
      const mapText = (value) => {
        let text = String(value ?? '');
        const replacements = [
          ['Starter Studio', 'Starter Studio / 新手工坊'], ['Forge Workbench', 'Forge Workbench / 锻造工坊'], ['Asset Yard', 'Asset Yard / 道具庭院'], ['ZBJ Market Gate', 'Bounty Market Gate / 悬赏集市门'], ['League Coliseum', 'League Coliseum / League 竞技场'], ['Mirror City Square', 'Mirror City Square / 镜像城市广场'], ['Guild Raid Hall', 'Guild Raid Hall / 公会团本厅'], ['Bounty Board', 'Bounty Board / 悬赏任务牌'], ['Result Rating Dock', 'Result Rating Dock / 成果评定台'], ['Dispute Desk', 'Dispute Desk / 争议柜台'],
          ['镜像城市广场', 'Mirror City Square / 镜像城市广场'], ['公会团本厅', 'Guild Raid Hall / 公会团本厅'], ['悬赏任务牌', 'Bounty Board / 悬赏任务牌'], ['成果评定台', 'Result Rating Dock / 成果评定台'], ['争议柜台', 'Dispute Desk / 争议柜台'], ['League 竞技场', 'League Arena / League 竞技场'],
          ['starter-studio', 'starter-studio / 新手工坊'], ['forge-workbench', 'forge-workbench / 锻造工坊'], ['asset-yard', 'asset-yard / 道具庭院'], ['zbj-market-gate', 'bounty-market-gate / 悬赏集市门'], ['league-coliseum', 'league-coliseum / League 竞技场'], ['cn-shanghai-core', 'global-start-zone / 全球首发区'],
          ['prefetch', 'prefetch / 预热分片'], ['street_nodes', 'street nodes / 街区节点'], ['neighbor_tile_warmup', 'neighbor warmup / 邻近地图预热'], ['warm', 'warm / 预热'], ['active', 'active / 活跃'], ['planned', 'planned / 规划中'], ['pending', 'pending / 待推进'], ['completed', 'completed / 已完成'], ['accepted', 'accepted / 已评级'], ['world_event', 'world event / 世界事件'], ['no-task', 'no task / 未关联任务'], ['dense', 'dense / 高密度'], ['regional', 'regional / 区域密度'],
          ['route_task', 'route task / 路线任务'], ['avatar_task_route', 'avatar task route / 角色任务路线'], ['avatar_route_runner', 'avatar route runner / 跑图角色'], ['Task routes', 'Task routes / 任务路线'], ['Avatar Task Routes', 'Avatar Task Routes / 角色任务路线'], ['Avatar Movement', 'Avatar Movement / 角色跑图'], ['Moving avatars', 'Moving avatars / 动态角色'], ['Trace route / 追踪路线', 'Trace route / 追踪路线'], ['Follow runner / 跟随角色', 'Follow runner / 跟随角色'], ['Complete checkpoint / 完成检查点', 'Complete checkpoint / 完成检查点'], ['Reward checkpoint / 奖励检查点', 'Reward checkpoint / 奖励检查点'], ['Checkpoint history / 检查点历史', 'Checkpoint history / 检查点历史'], ['Reward history / 奖励历史', 'Reward history / 奖励历史'], ['Agent party / Agent 小队', 'Agent party / Agent 小队'], ['Party state / 小队状态', 'Party state / 小队状态'], ['party members / 个小队成员', 'party members / 个小队成员'], ['Oracle Scout / 预判侦察', 'Oracle Scout / 预判侦察'], ['Forge Builder / 交付锻造', 'Forge Builder / 交付锻造'], ['Mirror Auditor / 镜像审计', 'Mirror Auditor / 镜像审计'], ['Courier Closer / 结算信使', 'Courier Closer / 结算信使'], ['history steps / 个历史节点', 'history steps / 个历史节点'], ['Ready to complete / 可完成', 'Ready to complete / 可完成'], ['Reward checkpoint locked / 奖励检查点未解锁', 'Reward checkpoint locked / 奖励检查点未解锁'], ['Approaching checkpoint / 接近检查点', 'Approaching checkpoint / 接近检查点'], ['Submit evidence → rating/reward / 提交证据 → 评级奖励', 'Submit evidence → rating/reward / 提交证据 → 评级奖励'], ['animated path / 动态路径', 'animated path / 动态路径'], ['looping_avatar_task_run', 'looping avatar task run / 循环任务跑图'], ['en_route_to_task_reward', 'en route to task reward / 正在跑向任务奖励'], ['ready_to_complete', 'ready to complete / 可完成'], ['route_started', 'route started / 路线开始'], ['evidence_checkpoint', 'evidence checkpoint / 证据检查点'], ['reward_settlement', 'rating/reward settlement / 评级奖励结算'], ['claimable_next', 'claimable next / 下一步可领取'], ['locked_until_checkpoint', 'locked until checkpoint / 检查点前锁定'], ['contract_capture', 'contract capture / 契约登记'], ['work_order', 'quest commission / 冒险委托'], ['delivery', 'result submit / 成果提交'], ['acceptance', 'rating pass / 评级'], ['rejection', 'revision / 返工'], ['reopen', 'reopen / 重开'], ['cancellation', 'cancel / 放弃'], ['live_event', 'live event / 实时事件'],
          ['poi', 'POI / 热点'], ['hub_square', 'hub square / 主城广场'], ['agent_home', 'Agent home / Agent 居所'], ['ledger_office', 'reward office / 奖励窗口'], ['workshop_room', 'workshop room / 工坊房间'], ['craft_station', 'craft station / 锻造台'], ['asset_yard', 'asset yard / 道具庭院'], ['market_gate', 'bounty gate / 悬赏入口'], ['client_board', 'quest board / 悬赏牌'], ['delivery_dock', 'rating dock / 成果评定台'], ['dispute_desk', 'dispute desk / 仲裁柜台'], ['arena_gate', 'arena gate / 竞技入口'], ['raid_hall', 'raid hall / 团本大厅'],
          ['customer-facing', 'player-facing / 玩家可用'], ['customer', 'client / 委托目标'], ['buyer', 'quest taker / 接取方'], ['seller', 'service party / 服务方'], ['commercial', 'market quest / 市场任务'], ['browser commerce E2E', 'browser adventure E2E / 浏览器冒险验收'], ['AI 设计公司', 'AI Design Studio / AI 设计工坊'], ['服务真实客户', 'serve real global clients / 完成海外真实委托'], ['真实客户', 'real global client / 海外真实委托'], ['委托方', 'client / 委托目标'],
          ['回访/升级悬赏', 'follow-up or upgrade bounty / 回访/升级悬赏'], ['升级悬赏', 'upgrade bounty / 升级悬赏'], ['回访', 'follow-up / 回访'], ['基于', 'based on / 基于'], ['提供', 'provide / 提供'], ['下一阶段', 'next-stage / 下一阶段'], ['赏金', 'bounty / 赏金'], ['推荐理由', 'recommendation rationale / 推荐理由'], ['评级后升级悬赏', 'post-rating bounty upgrade / 评级后升级悬赏'], ['评级通过', 'rating passed / 评级通过'], ['高阶范围', 'upgraded scope / 高阶范围'], ['赏金阶梯', 'bounty ladder / 赏金阶梯'], ['时间线', 'timeline / 时间线'], ['成果证据', 'result evidence / 成果证据'], ['委托方反馈', 'client feedback / 委托方反馈'], ['世界状态变化', 'world-state changes / 世界状态变化'], ['复盘', 'review / 复盘'], ['评级标准', 'rating criteria / 评级标准'], ['缺失证据', 'missing evidence / 缺失证据'], ['异议', 'objections / 异议'], ['委托目标', 'commission goal / 委托目标'], ['里程碑', 'milestone / 里程碑'], ['第一轮成果', 'first result / 第一轮成果'], ['事件', 'events / 事件'], ['委托', 'commissions / 委托'], ['契约', 'contracts / 契约'], ['战报', 'battle reports / 战报'], ['证据', 'evidence / 证据'], ['风险', 'risks / 风险'], ['目标', 'goals / 目标'], ['质量', 'quality / 质量'], ['成果', 'result / 成果'], ['声望奖励', 'reputation reward / 声望奖励'], ['记录', 'record / 记录'], ['确认', 'confirm / 确认'], ['输出', 'produce / 输出'], ['围绕', 'around / 围绕'],
          ['打开任务牌路线', 'Open bounty route / 打开任务牌路线'], ['打开契约路线', 'Open contract route / 打开契约路线'], ['打开契约捕捉路线', 'Open contract capture route / 打开契约捕捉路线'], ['打开契约完成路线', 'Open contract completion route / 打开契约完成路线'], ['打开成果提交路线', 'Open result submission route / 打开成果提交路线'], ['打开评级路线', 'Open rating route / 打开评级路线'], ['打开返工路线', 'Open revision route / 打开返工路线'], ['打开重开路线', 'Open reopen route / 打开重开路线'], ['打开放弃路线', 'Open cancellation route / 打开放弃路线'], ['打开关联契约', 'Open linked contract / 打开关联契约'], ['打开关联事件', 'Open linked event / 打开关联事件'], ['打开事件时间线', 'Open event timeline / 打开事件时间线'], ['打开事件线', 'Open event lane / 打开事件线'], ['起草世界行动', 'Draft world action / 起草世界行动'], ['起草任务后续', 'Draft task follow-up / 起草任务后续'], ['起草后续行动', 'Draft next action / 起草后续行动'], ['起草后续支线', 'Draft next branch / 起草后续支线'], ['推进委托', 'Advance commission / 推进委托'], ['推进下一条支线', 'Advance next branch / 推进下一条支线'], ['路线行动', 'Route action / 路线行动'], ['世界路线交接', 'World route handoff / 世界路线交接'], ['移动到这里', 'Move here / 移动到这里'],
          ['冒险路线', 'Adventure route / 冒险路线'], ['暂无地图焦点', 'no map focus / 暂无地图焦点'], ['未知地点', 'unknown place / 未知地点'], ['当前路线', 'current route / 当前路线'], ['当前世界路线', 'current world route / 当前世界路线'], ['下一条支线', 'next branch / 下一条支线'], ['支线', 'branch / 支线'], ['战果总结待生成。', 'Outcome summary pending. / 战果总结待生成。'], ['支线提示待生成。', 'Branch hint pending. / 支线提示待生成。'], ['支线打法待生成。', 'Branch playbook pending. / 支线打法待生成。'], ['继续推进下一步机会。', 'continue the next opportunity. / 继续推进下一步机会。'], ['围绕 contract 整理目标、证据、风险、评级标准和下一步。', 'prepare goals, evidence, risks, rating criteria, and next step around contract. / 围绕 contract 整理目标、证据、风险、评级标准和下一步。']
        ];
        replacements.forEach(([from, to]) => { text = text.replaceAll(from, to); });
        const localizeSlashPair = (source) => {
          if (!String(source || '').includes(' / ')) return source;
          const language = mapLanguage();
          const hasCjk = (piece) => /[\u3400-\u9fff\uf900-\ufaff]/.test(String(piece || ''));
          const hasLatin = (piece) => /[A-Za-z]/.test(String(piece || ''));
          const pieces = String(source || '').split(' / ').map((piece) => piece.trim()).filter(Boolean);
          const selected = language === 'zh'
            ? pieces.filter(hasCjk)
            : pieces.filter((piece) => hasLatin(piece) && !hasCjk(piece));
          return (selected.length ? selected : pieces).join(language === 'zh' ? '：' : ': ');
        };
        const normalizeLocalizedCopy = (source) => mapLanguage() === 'zh' ? source : String(source || '')
          .replaceAll('，', ',')
          .replaceAll('：', ':')
          .replaceAll('。', '.')
          .replaceAll('、', ', ');
        const stripResidualCjkForEnglish = (source) => mapLanguage() === 'zh' ? source : String(source || '')
          .replace(/[\u3400-\u9fff\uf900-\ufaff]/g, '')
          .replace(/[，：。；！？、]/g, ' ')
          .replace(/\s+/g, ' ')
          .trim();
        return stripResidualCjkForEnglish(normalizeLocalizedCopy(localizeSlashPair(text)));
      };
      const markerById = new Map((engine.markers || []).map((marker) => [String(marker.node_id || ''), marker]));
      const markerByLocationId = new Map();
      (engine.markers || []).forEach((marker) => {
        const locationId = String(marker.location_id || '').trim();
        if (locationId && !markerByLocationId.has(locationId)) markerByLocationId.set(locationId, marker);
      });
      const markerLayerById = new Map();
      const overlayLayers = {
        density: mapAdapter.createOverlayLayer(mapRuntime),
        regions: mapAdapter.createOverlayLayer(mapRuntime),
        tiles: mapAdapter.createOverlayLayer(mapRuntime),
        prefetch: mapAdapter.createOverlayLayer(mapRuntime),
        events: mapAdapter.createOverlayLayer(mapRuntime),
        taskRoutes: mapAdapter.createOverlayLayer(mapRuntime),
        routeRunners: mapAdapter.createOverlayLayer(mapRuntime),
        avatars: mapAdapter.createOverlayLayer(mapRuntime),
      };
      const overlayState = { density: true, regions: true, tiles: true, prefetch: true, events: true, taskRoutes: true, routeRunners: true, avatars: true };
      const overlayLabels = { density: 'Density / 密度', regions: 'Regions / 区域', tiles: 'Map tiles / 地图块', prefetch: 'Prefetch rings / 预热圈', events: 'Live events / 实时事件', taskRoutes: 'Task routes / 任务路线', routeRunners: 'Moving avatars / 动态角色', avatars: 'Player avatars / 跑图角色' };
"#
}

pub(super) fn real_world_map_runtime_primitives_js() -> &'static str {
    r#"      const tileToLatLng = (x, y, z) => {
        const scale = Math.pow(2, z);
        const lng = (x / scale) * 360 - 180;
        const latRadians = Math.atan(Math.sinh(Math.PI * (1 - (2 * y) / scale)));
        return { lat: (latRadians * 180) / Math.PI, lng };
      };
      const tileBoundsFromParts = (z, x, y) => {
        if (![z, x, y].every(Number.isFinite)) return null;
        const northWest = tileToLatLng(x, y, z);
        const southEast = tileToLatLng(x + 1, y + 1, z);
        return mapAdapter.latLngBounds([[northWest.lat, northWest.lng], [southEast.lat, southEast.lng]]);
      };
      const renderStreamHud = (viewport, focus = lastSelection) => {
        if (!streamHud) return;
        const density = mapText((viewport.player_density || {}).mode || 'dense');
        const lens = buildStreamLens(viewport, focus);
        const chips = [
          `<span class="hud-chip"><strong>${escapeHtml(viewport.stream_region_count ?? 0)}</strong> ${escapeHtml(mapText('regional shards / 个区域分片'))}</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.marker_count ?? 0)}</strong> ${escapeHtml(mapText('visible places / 个可见地点'))}</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.prefetch_count ?? 0)}</strong> ${escapeHtml(mapText('prefetch tiles / 个预热地图块'))}</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.live_event_count ?? 0)}</strong> ${escapeHtml(mapText('live events / 个实时事件'))} · ${escapeHtml(density)}</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.avatar_task_route_count ?? 0)}</strong> ${escapeHtml(mapText('task routes / 条任务路线'))}</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.avatar_route_runner_count ?? 0)}</strong> ${escapeHtml(mapText('moving avatars / 个动态角色'))}</span>`,
          `<span class="hud-chip"><strong>${escapeHtml(viewport.player_avatar_count ?? 0)}</strong> ${escapeHtml(mapText('running avatars / 个跑图角色'))}</span>`
        ];
        if (lens) {
          chips.push(`<span class="hud-chip"><strong>${escapeHtml(lens.count ?? 0)}</strong> ${escapeHtml(mapText('event lenses / 条事件镜头'))} · ${escapeHtml(mapText(lens.label || 'focus / 焦点'))}</span>`);
        }
        streamHud.innerHTML = chips.join('');
      };
      const setOverlayLayerVisibility = (name, enabled) => {
        const layer = overlayLayers[name];
        if (!layer) return;
        mapAdapter.setOverlayVisibility(mapRuntime, layer, enabled);
      };
      const renderOverlayStatus = () => {
        if (!overlayStatus) return;
        const active = Object.entries(overlayState)
          .filter(([, enabled]) => enabled)
          .map(([name]) => mapText(overlayLabels[name] || name));
        const activeRegion = mapText((((lastViewport || {}).active_region || {}).name) || 'Active region / 当前区域');
        overlayStatus.textContent = mapText('Active overlays / 当前图层') + ': ' + (active.length ? active.join(', ') : mapText('None / 无')) + ' · ' + mapText('Quick focus / 快捷焦点') + ': ' + activeRegion + ', ' + mapText('Nearest hotspot / 最近热点') + ', ' + mapText('Hottest event / 高热事件') + '.';
      };
      const refreshOverlayControls = () => {
        if (!overlayControls) return;
        overlayControls.querySelectorAll('.trillionnium-overlay-toggle').forEach((button) => {
          const enabled = !!overlayState[button.dataset.overlayTarget];
          button.setAttribute('aria-pressed', enabled ? 'true' : 'false');
          button.classList.toggle('is-off', !enabled);
        });
      };"#
}

pub(super) fn real_world_map_focus_core_js() -> &'static str {
    r#"      const buildDefaultFocus = () => {
        const activeRegion = (lastViewport || {}).active_region;
        if (activeRegion && activeRegion.center) {
          return {
            kind: 'region',
            lat: activeRegion.center.lat,
            lng: activeRegion.center.lng,
            zoom: activeRegion.zoom_max || 12,
          };
        }
        const hotspot = (((lastViewport || {}).poi_hotspots) || [])[0];
        if (hotspot) {
          return { kind: 'node', nodeId: hotspot.node_id };
        }
        return null;
      };
      const findLiveEventByFocus = (focus) => {
        if (!focus) return null;
        const eventId = String(focus.eventId || '').trim();
        const taskId = String(focus.taskId || '').trim();
        const locationId = String(focus.locationId || '').trim();
        const nodeId = String(focus.nodeId || '').trim();
        const stream = (((lastViewport || {}).live_event_stream) || []);
        return stream.find((eventItem) => eventId && String(eventItem.event_id || '').trim() === eventId)
          || stream.find((eventItem) => taskId && String(eventItem.cex_task_id || '').trim() === taskId && (!locationId || String(eventItem.location_id || '').trim() === locationId))
          || stream.find((eventItem) => locationId && String(eventItem.location_id || '').trim() === locationId && (!nodeId || String(eventItem.node_id || '').trim() === nodeId))
          || stream.find((eventItem) => nodeId && String(eventItem.node_id || '').trim() === nodeId)
          || null;
      };
      const buildEventFocus = (eventItem) => {
        const item = eventItem || {};
        const nodeId = String(item.node_id || '').trim();
        const marker = markerById.get(nodeId) || {};
        return {
          kind: 'event',
          nodeId,
          eventId: String(item.event_id || '').trim(),
          taskId: String(item.cex_task_id || '').trim(),
          locationId: String(item.location_id || marker.location_id || '').trim(),
          eventKind: String(item.event_kind || 'world_event'),
          nodeName: String(item.node_name || marker.name || item.location_id || 'POI'),
          eventBody: String(item.body || ''),
          eventResult: String(item.result || ''),
          suppressAction: true,
        };
      };
      const filterLiveEventStream = (stream, focus = lastSelection) => {
        const source = Array.isArray(stream) ? stream : [];
        if (!source.length) return source;
        const selection = buildSelectionFromFocus(focus);
        if (!selection) return source;
        const selectionTaskId = String(selection.taskId || '').trim();
        const selectionLocationId = String(selection.locationId || '').trim();
        const selectionEventId = String(selection.eventId || '').trim();
        let filtered = source;
        if (selectionTaskId) {
          filtered = source.filter((eventItem) => String(eventItem.cex_task_id || '').trim() === selectionTaskId);
        }
        if (!filtered.length && selectionLocationId) {
          filtered = source.filter((eventItem) => String(eventItem.location_id || '').trim() === selectionLocationId);
        }
        if (!filtered.length && selectionEventId) {
          filtered = source.filter((eventItem) => String(eventItem.event_id || '').trim() === selectionEventId);
        }
        return filtered.length ? filtered : source;
      };
      const buildStreamLens = (viewport, focus = lastSelection) => {
        const selection = buildSelectionFromFocus(focus);
        if (!selection) return null;
        const filtered = filterLiveEventStream((viewport || {}).live_event_stream || [], selection);
        if (!filtered.length) return null;
        const label = selection.kind === 'event'
          ? (selection.taskId ? ('任务 ' + selection.taskId) : (selection.title || '已选事件'))
            : (selection.title || selection.locationId || selection.nodeId || selection.kind || '焦点');
        return { count: filtered.length, label };
      };
      const filterAvatarTaskRoutes = (routes, focus = lastSelection) => {
        const source = Array.isArray(routes) ? routes : [];
        if (!source.length) return source;
        const selection = buildSelectionFromFocus(focus);
        if (!selection) return source;
        const selectionTaskId = String(selection.taskId || '').trim();
        const selectionLocationId = String(selection.locationId || '').trim();
        const selectionNodeId = String(selection.nodeId || '').trim();
        let filtered = source;
        if (selectionTaskId) {
          filtered = source.filter((route) => String(route.task_id || '').trim() === selectionTaskId);
        }
        if (!filtered.length && selectionNodeId) {
          filtered = source.filter((route) => String(route.to_node_id || '').trim() === selectionNodeId || String(route.from_node_id || '').trim() === selectionNodeId);
        }
        if (!filtered.length && selectionLocationId) {
          filtered = source.filter((route) => String(route.latest_location_id || '').trim() === selectionLocationId);
        }
        return filtered.length ? filtered : source;
      };
      const filterAvatarRouteRunners = (runners, focus = lastSelection) => filterAvatarTaskRoutes(runners, focus);
      const findRegionByFocus = (focus) => {
        const focusLat = Number((focus || {}).lat);
        const focusLng = Number((focus || {}).lng);
        const regions = [
          ...((((lastViewport || {}).stream_region_shards) || [])),
          ((lastViewport || {}).active_region),
          ...((engine.region_shards) || []),
        ].filter(Boolean);
        return regions.find((region) => {
          const centerPoint = region.center || {};
          return Math.abs(Number(centerPoint.lat || 0) - focusLat) < 0.00001 && Math.abs(Number(centerPoint.lng || 0) - focusLng) < 0.00001;
        }) || null;
      };
"#
}

pub(super) fn real_world_map_selection_location_ids_js() -> &'static str {
    r#"      const resolveSelectionLocationIds = (focus) => {
        const locationIds = new Set();
        if (!focus) return locationIds;
        if (focus.kind === 'node') {
          const marker = markerById.get(String(focus.nodeId || ''));
          if (marker && marker.location_id) locationIds.add(String(marker.location_id));
          return locationIds;
        }
        if (focus.kind === 'event') {
          const eventItem = findLiveEventByFocus(focus) || {};
          const locationId = String(eventItem.location_id || focus.locationId || '').trim();
          if (locationId) locationIds.add(locationId);
          const marker = markerById.get(String(eventItem.node_id || focus.nodeId || '').trim());
          if (marker && marker.location_id) locationIds.add(String(marker.location_id));
          return locationIds;
        }
        if (focus.kind === 'tile') {
          const tile = buildSelectionFromFocus(focus) || {};
          (tile.nodeIds || []).forEach((nodeId) => {
            const marker = markerById.get(String(nodeId || ''));
            if (marker && marker.location_id) locationIds.add(String(marker.location_id));
          });
          return locationIds;
        }
        ((((lastViewport || {}).visible_markers) || [])).forEach((marker) => {
          if (marker && marker.location_id) locationIds.add(String(marker.location_id));
        });
        return locationIds;
      };"#
}

pub(super) fn real_world_map_selection_builder_js() -> &'static str {
    r#"      const buildSelectionFromFocus = (focus) => {
        if (!focus) return null;
        if (focus.kind === 'node') {
          const marker = markerById.get(String(focus.nodeId || ''));
          if (!marker) return null;
          return {
            kind: 'node',
            title: mapText(marker.name || marker.node_id || '热点'),
            summary: mapText(marker.node_kind || '热点') + ' · ' + (mapText(((marker.interaction_tags || []).slice(0, 3)).join(' / ')) || '世界互动'),
            detail: mapText(marker.description || '从这个热点移动、查看、协作、制作，或开启世界行动。'),
            nodeId: marker.node_id,
            locationId: focus.locationId || marker.location_id || '',
            taskId: String(focus.taskId || '').trim(),
            actions: marker.primary_actions || [],
          };
        }
        if (focus.kind === 'event') {
          const eventItem = findLiveEventByFocus(focus) || {};
          const nodeId = String(eventItem.node_id || focus.nodeId || '').trim();
          const marker = markerById.get(nodeId) || {};
          const taskId = String(eventItem.cex_task_id || focus.taskId || '').trim();
          const locationId = String(eventItem.location_id || focus.locationId || marker.location_id || '').trim();
          const impact = Number(eventItem.impact_score ?? focus.impact ?? 0);
          const eventKind = mapText(String(eventItem.event_kind || focus.eventKind || 'world_event'));
          const nodeName = mapText(String(eventItem.node_name || focus.nodeName || marker.name || locationId || '热点'));
          return {
            kind: 'event',
            title: eventKind + ' · ' + nodeName,
            summary: (taskId ? ('任务 ' + taskId) : '未关联的实时事件') + ' · 影响 ' + (Number.isFinite(impact) ? impact : 0),
            detail: mapText(String(eventItem.result || focus.eventResult || eventItem.body || focus.eventBody || '把这个实时事件推进到冒险路线和下一步世界行动。')),
            nodeId,
            locationId,
            taskId,
            eventId: String(eventItem.event_id || focus.eventId || '').trim(),
            eventBody: String(eventItem.body || focus.eventBody || '').trim(),
            eventResult: String(eventItem.result || focus.eventResult || '').trim(),
            actions: marker.primary_actions || [],
          };
        }
        if (focus.kind === 'region') {
          const region = findRegionByFocus(focus) || {};
          return {
            kind: 'region',
            title: mapText(region.name || '区域焦点'),
            summary: mapText(region.status || '规划中') + ' · ' + mapText(region.coverage_kind || '分片'),
            detail: '缩放 ' + (region.zoom_min || focus.zoom || 12) + '-' + (region.zoom_max || focus.zoom || 12) + ' · 密度 ' + mapText(region.player_density_mode || (((lastViewport || {}).player_density || {}).mode) || '混合'),
            lat: focus.lat,
            lng: focus.lng,
            zoom: region.zoom_max || focus.zoom || 12,
          };
        }
        if (focus.kind === 'tile') {
          const tiles = [
            ...((((lastViewport || {}).visible_tile_shards) || [])),
            ...((((lastViewport || {}).prefetch_queue) || [])),
          ];
          const tile = tiles.find((item) => String(item.z || '') === String(focus.z || '') && String(item.x || '') === String(focus.x || '') && String(item.y || '') === String(focus.y || '')) || {};
          return {
            kind: 'tile',
            title: tile.tile_id || '地图分片',
            summary: mapText(tile.tile_status || '地图块') + ' · ' + String(tile.marker_count ?? 0) + ' 个地点',
            detail: mapText(tile.lod_mode || '街区节点') + ' · 地图块 ' + [focus.z, focus.x, focus.y].filter((value) => value !== undefined && value !== null && value !== '').join('/'),
            nodeIds: tile.node_ids || [],
            z: focus.z,
            x: focus.x,
            y: focus.y,
          };
        }
        return null;
      };
"#
}

pub(super) fn real_world_map_selection_signal_js() -> &'static str {
    r#"      const selectionEventSignalText = (selection) => {
        if (!selection || selection.kind !== 'event') return '';
        const result = String(selection.eventResult || '').trim();
        const body = String(selection.eventBody || '').trim();
        if (result && body) return '最新事件信号：' + mapText(result) + ' · ' + mapText(body);
        if (result) return '最新事件信号：' + mapText(result);
        if (body) return '最新事件信号：' + mapText(body);
        return '';
      };
      const appendSelectionEventSignal = (body, selection) => {
        const eventSignal = selectionEventSignalText(selection);
        return eventSignal ? (body + ' ' + eventSignal) : body;
      };"#
}

pub(super) fn real_world_map_focus_panel_js() -> &'static str {
    r#"      const selectionActionButtonHtml = (attrs, label, extraAttrs = '') => {
        const attrHtml = Object.entries(attrs || {})
          .filter(([, value]) => value !== undefined && value !== null)
          .map(([name, value]) => ` data-${name}="${escapeHtml(value)}"`)
          .join('');
        return `<button type="button" class="focus-chip trillionnium-selection-action"${attrHtml}${extraAttrs}>${escapeHtml(mapText(label || 'Action / 行动'))}</button>`;
      };
      const selectionCameraActionButtonHtml = (actionId, label) => selectionActionButtonHtml({ 'selection-kind': 'camera', 'camera-action': actionId }, label);
      const buildMapFocusActionButtonsHtml = (selection, options) => {
        const nodeButtonExtraAttrs = String(((options || {}).nodeButtonExtraAttrs) || '');
        const buttons = [];
        if (selection.kind === 'node' || selection.kind === 'event') {
          buttons.push(...(selection.actions || []).map((action) => selectionActionButtonHtml({ 'selection-kind': 'node', 'node-id': selection.nodeId || '', 'action-id': action.action_id || 'move_here' }, action.label || action.command || '行动', nodeButtonExtraAttrs)));
        } else if (selection.kind === 'region') {
          buttons.push(selectionActionButtonHtml({ 'selection-kind': 'region', lat: selection.lat ?? '', lng: selection.lng ?? '', zoom: selection.zoom ?? 12 }, 'Focus region / 聚焦区域'));
          buttons.push(selectionCameraActionButtonHtml('nearest_poi', 'Nearest hotspot / 最近热点'));
          buttons.push(selectionCameraActionButtonHtml('hottest_event', 'Hottest event / 高热事件'));
        } else if (selection.kind === 'tile') {
          buttons.push(selectionActionButtonHtml({ 'selection-kind': 'tile', 'tile-z': selection.z ?? '', 'tile-x': selection.x ?? '', 'tile-y': selection.y ?? '' }, 'View tile / 查看分片'));
          buttons.push(selectionCameraActionButtonHtml('nearest_poi', 'Nearest hotspot / 最近热点'));
          buttons.push(selectionCameraActionButtonHtml('hottest_event', 'Hottest event / 高热事件'));
        }
        return buttons.join(' ');
      };
      const renderMapFocusPanel = (options) => {
        const focusSummaryNode = (options || {}).focusSummary;
        const focusDetailNode = (options || {}).focusDetail;
        const actionRailNode = (options || {}).actionRail;
        if (!focusSummaryNode || !focusDetailNode || !actionRailNode) return null;
        const focus = ((options || {}).focus) || buildDefaultFocus();
        const selection = buildSelectionFromFocus(focus);
        if (!selection) {
          focusSummaryNode.textContent = mapText(String(((options || {}).emptySummary) || 'Waiting for map focus / 等待选择地图焦点…'));
          focusDetailNode.textContent = mapText(String(((options || {}).emptyDetail) || 'Choose a region, tile, hotspot, or live event to drive movement and world action / 选择区域、地图块、热点或实时事件，推动移动和世界行动。'));
          actionRailNode.innerHTML = '';
          if (typeof (options || {}).onEmpty === 'function') options.onEmpty();
          return null;
        }
        focusSummaryNode.textContent = mapText(selection.title || 'Map focus / 地图焦点');
        focusDetailNode.textContent = mapText(selection.summary || 'World focus / 世界焦点') + ' · ' + mapText(selection.detail || '');
        actionRailNode.innerHTML = buildMapFocusActionButtonsHtml(selection, options);
        if (typeof (options || {}).onRendered === 'function') options.onRendered(selection);
        return selection;
      };"#
}

pub(super) fn real_world_map_focus_camera_js() -> &'static str {
    r#"      const focusMapSurface = (focus) => {
        if (!focus) return;
        if (focus.kind === 'node') {
          const marker = markerById.get(String(focus.nodeId || ''));
          if (!marker) return;
          mapAdapter.focus(mapRuntime, { lat: marker.lat, lng: marker.lng, zoom: Number(focus.zoom) || Math.max(mapAdapter.getZoom(mapRuntime), 16) });
          const markerLayer = markerLayerById.get(String(marker.node_id || ''));
          if (markerLayer) markerLayer.openPopup();
          if (!focus.suppressAction) {
            window.trillionniumApplyMarkerAction(marker.node_id || focus.nodeId, 'move_here');
          }
          return;
        }
        if (focus.kind === 'event') {
          const eventItem = findLiveEventByFocus(focus) || {};
          const marker = markerById.get(String(eventItem.node_id || focus.nodeId || ''));
          if (!marker) return;
          mapAdapter.focus(mapRuntime, { lat: marker.lat, lng: marker.lng, zoom: Number(focus.zoom) || Math.max(mapAdapter.getZoom(mapRuntime), 16) });
          const markerLayer = markerLayerById.get(String(marker.node_id || ''));
          if (markerLayer) markerLayer.openPopup();
          if (!focus.suppressAction) {
            window.trillionniumApplyMarkerAction(marker.node_id || focus.nodeId, 'move_here');
          }
          return;
        }
        if (focus.kind === 'region') {
          const lat = Number(focus.lat);
          const lng = Number(focus.lng);
          if (!Number.isFinite(lat) || !Number.isFinite(lng)) return;
          mapAdapter.focus(mapRuntime, { lat, lng, zoom: Number(focus.zoom) || 12 });
          return;
        }
        if (focus.kind === 'tile') {
          const bounds = tileBoundsFromParts(Number(focus.z), Number(focus.x), Number(focus.y));
          if (!bounds) return;
          mapAdapter.fitBounds(mapRuntime, bounds.pad(0.18));
        }
      };
      const runCameraAction = (actionId) => {
        if (!lastViewport) return;
        if (actionId === 'active_region') {
          const region = lastViewport.active_region || (lastViewport.stream_region_shards || [])[0];
          if (!region) return;
          const centerPoint = region.center || {};
          const focus = { kind: 'region', lat: centerPoint.lat, lng: centerPoint.lng, zoom: region.zoom_max || 12 };
          focusMapSurface(focus);
          setFocusSelection(focus);
          if (cameraSummary) cameraSummary.textContent = mapText('Quick focus / 快捷焦点') + ': ' + mapText('Active region / 当前区域') + ' · ' + mapText(region.name || region.region_id || 'region');
          return;
        }
        if (actionId === 'nearest_poi') {
          const hotspot = (lastViewport.poi_hotspots || [])[0];
          if (!hotspot) return;
          const focus = { kind: 'node', nodeId: hotspot.node_id };
          focusMapSurface(focus);
          setFocusSelection(focus);
          if (cameraSummary) cameraSummary.textContent = mapText('Quick focus / 快捷焦点') + ': ' + mapText('Nearest hotspot / 最近热点') + ' · ' + mapText(hotspot.name || hotspot.node_id || 'poi');
          return;
        }
        if (actionId === 'hottest_event') {
          const hottestEvent = [...(lastViewport.live_event_stream || [])].sort((left, right) => Number(right.impact_score || 0) - Number(left.impact_score || 0))[0];
          if (!hottestEvent) return;
          const focus = { kind: 'event', nodeId: hottestEvent.node_id, eventId: hottestEvent.event_id, taskId: hottestEvent.cex_task_id, locationId: hottestEvent.location_id, eventKind: hottestEvent.event_kind, nodeName: hottestEvent.node_name, eventBody: hottestEvent.body, eventResult: hottestEvent.result, impact: hottestEvent.impact_score, suppressAction: true };
          focusMapSurface(focus);
          setFocusSelection(focus);
          if (cameraSummary) cameraSummary.textContent = mapText('Quick focus / 快捷焦点') + ': ' + mapText('Hottest event / 高热事件') + ' · ' + mapText(hottestEvent.event_kind || 'world_event') + ' · ' + mapText(hottestEvent.node_name || hottestEvent.node_id || 'event');
        }
      };
"#
}

pub(super) fn real_world_map_static_marker_layers_js() -> &'static str {
    r#"      const mapMarkerActionButtonHtml = (marker, action) => `<button type="button" class="trillionnium-map-action" data-node-id="${escapeHtml(marker.node_id)}" data-action-id="${escapeHtml(action.action_id || 'move_here')}">${escapeHtml(mapText(action.label || action.command || 'Action / 行动'))}</button>`;
      (engine.route_edges || []).forEach((edge) => {
        if (!edge.from || !edge.to) return;
        mapAdapter.renderRouteLine(mapRuntime, edge.from, edge.to, { color: '#64e3ff', weight: 2, opacity: 0.62 });
      });
      (engine.markers || []).forEach((marker) => {
        const actionButtons = (marker.primary_actions || []).map((action) => mapMarkerActionButtonHtml(marker, action)).join(' ');
        const actions = (marker.primary_actions || []).map((action) => `<br><code>${escapeHtml(action.command || action.label || '')}</code>`).join('');
        const popup = `<strong>${escapeHtml(marker.name)}</strong><br><code>${escapeHtml(marker.node_id)}</code><br>${escapeHtml(marker.description)}${actions}<br>${actionButtons}`;
        const markerLayer = mapAdapter.renderPoiMarker(mapRuntime, marker, popup);
        markerLayerById.set(String(marker.node_id || ''), markerLayer);
      });"#
}

pub(super) fn real_world_map_click_action_helpers_js() -> &'static str {
    r#"      const closestFromEvent = (event, selector) => event && event.target && event.target.closest ? event.target.closest(selector) : null;
      const mapClickSelectors = Object.freeze({
        action: '.trillionnium-map-action',
        overlay: '.trillionnium-overlay-toggle',
        selection: '.trillionnium-selection-action',
        camera: '.trillionnium-map-camera-action',
        focus: '.trillionnium-map-focus',
      });
      const handleMapActionButton = (button) => {
        if (!button) return false;
        window.trillionniumApplyMarkerAction(button.dataset.nodeId, button.dataset.actionId || 'move_here');
        return true;
      };
      const handleSelectionActionButton = (button, options = {}) => {
        if (!button) return false;
        const selectionKind = button.dataset.selectionKind;
        if (selectionKind === 'node') {
          const focus = buildSelectionFocusFromButton(button);
          focusMapSurface(focus);
          const handoff = window.trillionniumApplyMarkerAction(button.dataset.nodeId, button.dataset.actionId || 'move_here');
          if (typeof options.afterNodeAction === 'function') options.afterNodeAction(button, handoff);
          return true;
        }
        if (selectionKind === 'region' || selectionKind === 'tile') {
          const focus = buildSelectionFocusFromButton(button);
          focusMapSurface(focus);
          setFocusSelection(focus);
          return true;
        }
        if (selectionKind === 'camera') {
          runCameraAction(button.dataset.cameraAction);
          return true;
        }
        return false;
      };
      const handleMapCameraActionButton = (button) => {
        if (!button) return false;
        runCameraAction(button.dataset.cameraAction);
        return true;
      };
      const handleMapFocusButton = (button) => {
        if (!button) return false;
        const focus = buildMapFocusFromButton(button);
        focusMapSurface(focus);
        setFocusSelection(focus);
        return true;
      };"#
}

pub(super) fn real_world_map_overlay_render_js() -> &'static str {
    r#"      const renderViewportOverlays = (viewport) => {
        Object.values(overlayLayers).forEach((layer) => mapAdapter.clearOverlay(layer));
        const density = viewport.player_density || {};
        const densityRadiusMeters = density.mode === 'dense' ? 700 : density.mode === 'regional' ? 2800 : density.mode === 'sparse' ? 12000 : 30000;
        if (viewport.center && Number.isFinite(viewport.center.lat) && Number.isFinite(viewport.center.lng)) {
          mapAdapter.renderDensityCircle(overlayLayers.density, viewport.center, densityRadiusMeters, { color: '#64e3ff', weight: 1.2, fillColor: '#64e3ff', fillOpacity: 0.05 });
        }
        (viewport.stream_region_shards || []).forEach((region) => {
          const centerPoint = region.center || {};
          if (!Number.isFinite(centerPoint.lat) || !Number.isFinite(centerPoint.lng)) return;
          const stroke = region.status === 'active' ? '#f8c35b' : (region.status === 'warm' ? '#64e3ff' : '#8d97a6');
          mapAdapter.renderRegionAnchor(overlayLayers.regions, centerPoint, { radius: region.status === 'active' ? 9 : 7, color: stroke, fillColor: stroke, fillOpacity: 0.28, weight: 1.6 })
            .bindTooltip(`${mapText(region.name || '区域')} · ${mapText(region.status || '规划中')}`);
        });
        (viewport.visible_tile_shards || []).forEach((tile) => {
          const bounds = tileBoundsFromParts(Number(tile.z), Number(tile.x), Number(tile.y));
          if (!bounds) return;
          const active = tile.tile_status === 'active';
          mapAdapter.renderTileFrame(overlayLayers.tiles, bounds, { color: active ? '#64e3ff' : '#34506d', weight: active ? 2 : 1, fillColor: active ? '#64e3ff' : '#203244', fillOpacity: active ? 0.12 : 0.02 })
            .bindTooltip(`${tile.tile_id || '地图块'} · ${tile.marker_count ?? 0} 个地点`);
        });
        (viewport.prefetch_queue || []).forEach((tile) => {
          const bounds = tileBoundsFromParts(Number(tile.z), Number(tile.x), Number(tile.y));
          if (!bounds) return;
          mapAdapter.renderTileFrame(overlayLayers.prefetch, bounds, { color: '#f8c35b', weight: 2, dashArray: '6 6', fillColor: '#f8c35b', fillOpacity: 0.04 })
            .bindTooltip(`预热 · ${mapText(tile.priority_label || 'warm')} · ${tile.tile_id || '地图块'}`);
        });
        const visibleMarkerById = new Map((viewport.visible_markers || []).map((marker) => [String(marker.node_id || ''), marker]));
        (viewport.live_event_stream || []).forEach((eventItem) => {
          const marker = visibleMarkerById.get(String(eventItem.node_id || '')) || markerById.get(String(eventItem.node_id || ''));
          if (!marker) return;
          const impact = Number(eventItem.impact_score || 0);
          const eventFocus = buildEventFocus(eventItem);
          mapAdapter.renderEventPulse(overlayLayers.events, marker, { radius: Math.max(6, Math.min(12, 5 + impact / 3)), color: '#ff8d4d', fillColor: '#ff8d4d', fillOpacity: 0.3, weight: 1.4 })
            .bindTooltip(`${mapText(eventItem.event_kind || 'world_event')} · ${mapText(eventItem.node_name || marker.name || eventItem.location_id || '热点')}`)
            .on('click', () => {
              focusMapSurface(eventFocus);
              setFocusSelection(eventFocus);
            });
        });
        (viewport.avatar_task_routes || []).forEach((route) => {
          const from = route.from || {};
          const to = route.to || {};
          if (!Number.isFinite(Number(from.lat)) || !Number.isFinite(Number(from.lng)) || !Number.isFinite(Number(to.lat)) || !Number.isFinite(Number(to.lng))) return;
          const routeLabel = `${mapText(route.task_id || 'task route')} · ${mapText(route.next_action_label || 'Next action')}`;
          const sameNode = String(route.from_node_id || '') === String(route.to_node_id || '');
          const routeFocus = { kind: 'node', nodeId: route.to_node_id, taskId: route.task_id, locationId: route.latest_location_id, suppressAction: true };
          const layer = sameNode
            ? mapAdapter.renderEventPulse(overlayLayers.taskRoutes, to, { radius: 10, color: '#a78bfa', fillColor: '#a78bfa', fillOpacity: 0.22, weight: 2, className: 'trillionnium-avatar-task-route-pulse' })
            : mapAdapter.renderRouteLine(overlayLayers.taskRoutes, from, to, { color: '#a78bfa', weight: 4, opacity: 0.86, dashArray: '4 8', className: 'trillionnium-avatar-task-route-path' });
          layer.bindTooltip(`${routeLabel} · ${mapText(route.reward_loop || 'move → task → reward')}`)
            .on('click', () => {
              focusMapSurface(routeFocus);
              setFocusSelection(routeFocus);
            });
        });
        (viewport.avatar_route_runners || []).forEach((runner) => {
          const from = runner.from || {};
          const to = runner.to || {};
          const checkpoint = runner.reward_checkpoint || {};
          const runnerTracePoints = Array.isArray(runner.runner_trace_points) ? runner.runner_trace_points : [];
          const traceCount = runnerTracePoints.length;
          if (!Number.isFinite(Number(from.lat)) || !Number.isFinite(Number(from.lng)) || !Number.isFinite(Number(to.lat)) || !Number.isFinite(Number(to.lng))) return;
          const current = runner.current || {};
          const hasCurrent = Number.isFinite(Number(current.lat)) && Number.isFinite(Number(current.lng));
          const sameNode = Math.abs(Number(from.lat) - Number(to.lat)) < 0.000001 && Math.abs(Number(from.lng) - Number(to.lng)) < 0.000001;
          if (hasCurrent && !sameNode) {
            mapAdapter.renderRouteLine(overlayLayers.routeRunners, from, current, { color: '#64e3ff', weight: 5, opacity: 0.9, className: 'trillionnium-avatar-route-runner-progress' })
              .bindTooltip(`${mapText(runner.progress_label || 'route progress / 路线进度')} · ${mapText(runner.eta_label || 'ETA / 预计')}`);
            mapAdapter.renderRouteLine(overlayLayers.routeRunners, current, to, { color: '#a78bfa', weight: 3, opacity: 0.56, dashArray: '3 9', className: 'trillionnium-avatar-route-runner-remaining' })
              .bindTooltip(`${mapText(runner.arrival_label || 'Reward checkpoint / 奖励检查点')} · ${escapeHtml(Number(runner.remaining_distance_meters || 0))}m`);
          }
          const runnerFocus = { kind: 'node', nodeId: runner.to_node_id, taskId: runner.task_id, locationId: runner.latest_location_id, suppressAction: true };
          const checkpointReady = Boolean(checkpoint.ready);
          const checkpointLabel = mapText(checkpoint.label || runner.completion_label || 'Reward checkpoint / 奖励检查点');
          mapAdapter.renderEventPulse(overlayLayers.routeRunners, to, { radius: checkpointReady ? 13 : 9, color: checkpointReady ? '#8dffb0' : '#f8c35b', fillColor: checkpointReady ? '#8dffb0' : '#f8c35b', fillOpacity: checkpointReady ? 0.34 : 0.2, weight: 2, className: 'trillionnium-avatar-route-reward-checkpoint' })
            .bindTooltip(`${checkpointLabel} · ${mapText(checkpoint.reward_claim_label || 'Submit evidence → rating/reward / 提交证据 → 评级奖励')}`)
            .on('click', () => {
              focusMapSurface(runnerFocus);
              setFocusSelection(runnerFocus);
          });
          const completionButton = routeRunnerCompletionButtonHtml(runner, 'trillionnium-route-flow-action trillionnium-app-route-flow-action');
          const historyChips = routeRunnerHistoryChipsHtml(runner);
          const partyChips = agentPartyChipsHtml(runner);
          const popupHtml = `<strong>${escapeHtml(mapText(runner.movement_label || 'Avatar running to task / 角色正在跑向任务'))}</strong><br/><span>${escapeHtml(mapText(runner.from_node_name || runner.from_node_id || 'avatar'))} → ${escapeHtml(mapText(runner.to_node_name || runner.to_node_id || 'task'))}</span><br/><span>${escapeHtml(mapText(runner.progress_label || 'route progress / 路线进度'))} · ${escapeHtml(mapText(runner.eta_label || 'ETA / 预计'))} · ${traceCount} ${escapeHtml(mapText('trace points / 个追踪点'))}</span><br/><span>${escapeHtml(checkpointLabel)} · ${escapeHtml(mapText(runner.completion_label || 'Complete checkpoint / 完成检查点'))}</span><br/><code>${escapeHtml(runner.completion_command || '')}</code><br/><div class="focus-stack">${completionButton} ${historyChips} ${partyChips}</div><small>${escapeHtml(mapText(runner.checkpoint_history_summary || runner.reward_loop || 'move → task → reward / 移动 → 任务 → 奖励'))}</small>`;
          const runnerLayer = mapAdapter.renderMovingAvatar(overlayLayers.routeRunners, runner, popupHtml);
          if (runnerLayer) {
            runnerLayer.bindTooltip(`${mapText(runner.movement_label || 'Avatar running to task')} · ${mapText(runner.next_action_label || runner.task_id || 'task')}`)
              .on('click', () => {
                focusMapSurface(runnerFocus);
                setFocusSelection(runnerFocus);
              });
          }
        });
        (viewport.player_avatars || []).forEach((avatar) => {
          if (!Number.isFinite(Number(avatar.lat)) || !Number.isFinite(Number(avatar.lng))) return;
          const marker = markerById.get(String(avatar.node_id || '')) || {};
          const partyChips = agentPartyChipsHtml(avatar);
          const popupHtml = `<strong>${escapeHtml(mapText(avatar.display_name || avatar.matrix_user_id || 'Player avatar / 玩家角色'))}</strong><br/><span>${escapeHtml(mapText(avatar.node_name || marker.name || avatar.node_id || 'World node / 世界节点'))}</span><br/><div class="focus-stack">${partyChips}</div><small>${escapeHtml(mapText(avatar.task_loop || 'move → task → reward / 移动 → 任务 → 奖励'))}</small>`;
          mapAdapter.renderPlayerAvatar(overlayLayers.avatars, avatar, popupHtml)
            .on('click', () => {
              focusMapSurface({ kind: 'node', nodeId: avatar.node_id, suppressAction: true });
              setFocusSelection({ kind: 'node', nodeId: avatar.node_id, suppressAction: true });
            });
        });
        if (overlayLegend) {
          const regionName = mapText(((viewport.active_region || {}).name) || '区域');
          overlayLegend.textContent = '图层说明：' + regionName + ' 锚点 · ' + (viewport.tile_shard_count || 0) + ' 个地图块 · ' + (viewport.prefetch_count || 0) + ' 个预热圈 · ' + (viewport.live_event_count || 0) + ' 个实时事件脉冲 · ' + (viewport.avatar_task_route_count || 0) + ' 条任务路线 · ' + (viewport.avatar_route_runner_count || 0) + ' 个动态角色 · ' + (viewport.player_avatar_count || 0) + ' 个跑图角色。';
        }
        Object.keys(overlayLayers).forEach((name) => setOverlayLayerVisibility(name, overlayState[name] !== false));
      };
      const handleOverlayToggleButton = (button) => {
        const key = button && button.dataset ? button.dataset.overlayTarget : '';
        if (!Object.prototype.hasOwnProperty.call(overlayState, key)) return false;
        overlayState[key] = !overlayState[key];
        setOverlayLayerVisibility(key, overlayState[key]);
        refreshOverlayControls();
        renderOverlayStatus();
        return true;
      };
"#
}

pub(super) fn real_world_map_card_focus_helpers_js() -> &'static str {
    r#"      const mapFocusButtonAttrs = (attrs) => Object.entries(attrs || {})
        .filter(([, value]) => value !== undefined && value !== null)
        .map(([name, value]) => ` data-${name}="${escapeHtml(value)}"`)
        .join('');
      const mapRegionFocusButton = (item, label = 'Focus region / 聚焦区域') => {
        const centerPoint = (item && item.center) || {};
        return `<button type="button" class="focus-chip trillionnium-map-focus"${mapFocusButtonAttrs({ 'focus-kind': 'region', lat: centerPoint.lat ?? '', lng: centerPoint.lng ?? '', zoom: (item || {}).zoom_max ?? 12 })}>${escapeHtml(mapText(label))}</button>`;
      };
      const mapTileFocusButton = (item, label = 'View map tile / 查看地图分片') => `<button type="button" class="focus-chip trillionnium-map-focus"${mapFocusButtonAttrs({ 'focus-kind': 'tile', 'tile-z': (item || {}).z ?? '', 'tile-x': (item || {}).x ?? '', 'tile-y': (item || {}).y ?? '' })}>${escapeHtml(mapText(label))}</button>`;
      const mapNodeFocusButton = (item, label = 'Focus hotspot / 聚焦热点') => `<button type="button" class="focus-chip trillionnium-map-focus"${mapFocusButtonAttrs({ 'focus-kind': 'node', 'node-id': (item || {}).node_id || '', 'task-id': (item || {}).task_id || '', 'location-id': (item || {}).location_id || '', 'suppress-action': (item || {}).suppress_action ? 'true' : '' })}>${escapeHtml(mapText(label))}</button>`;
      const mapAvatarTaskRouteFocusButton = (item, label = 'Trace route / 追踪路线') => {
        const route = item || {};
        return mapNodeFocusButton({ node_id: route.to_node_id || '', task_id: route.task_id || '', location_id: route.latest_location_id || '', suppress_action: true }, label);
      };
      const mapEventFocusButton = (item, label = 'Track event / 追踪事件') => {
        const eventItem = item || {};
        return `<button type="button" class="focus-chip trillionnium-map-focus"${mapFocusButtonAttrs({ 'focus-kind': 'event', 'node-id': eventItem.node_id || '', 'event-id': eventItem.event_id || '', 'task-id': eventItem.cex_task_id || '', 'location-id': eventItem.location_id || '', 'event-kind': eventItem.event_kind || 'world_event', 'node-name': eventItem.node_name || eventItem.location_id || 'POI', 'event-body': eventItem.body || '', 'event-result': eventItem.result || '', 'suppress-action': 'true' })}>${escapeHtml(mapText(label))}</button>`;
      };
      const mapViewportCardModel = (item, kind, style = 'app') => {
        const source = item || {};
        const worldStyle = style === 'world';
        if (kind === 'region') {
          return {
            className: worldStyle ? 'mini shard' : 'module',
            title: mapText(source.name || 'Region / 区域'),
            meta: worldStyle
              ? `${mapText(source.status || 'planned / 规划中')} · ${mapText(source.coverage_kind || 'shard / 分片')} · ${source.distance_km ?? 0} km`
              : `${mapText(source.status || 'planned / 规划中')} · ${source.distance_km ?? 0} km`,
            code: source.region_id || 'region',
            focusHtml: mapRegionFocusButton(source),
          };
        }
        if (kind === 'tile') {
          return {
            className: worldStyle ? 'mini tile' : 'module',
            title: mapText(source.tile_status || 'map tile / 地图块'),
            meta: `${mapText(source.lod_mode || 'LOD')} · ${source.marker_count ?? 0} ${mapText('locations / 个地点')}`,
            code: source.tile_id || '地图块',
            focusHtml: mapTileFocusButton(source, 'View tile / 查看分片'),
          };
        }
        if (kind === 'prefetch') {
          return {
            className: worldStyle ? 'mini prefetch' : 'module',
            title: mapText(source.priority_label || 'warm / 预热'),
            meta: `${mapText(source.prefetch_reason || 'neighbor tile warmup / 邻近地图块预热')} · ${source.marker_count ?? 0} ${mapText('locations / 个地点')}`,
            code: source.tile_id || 'map tile / 地图块',
            focusHtml: mapTileFocusButton(source, 'Warm tile / 预热分片'),
          };
        }
        if (kind === 'event') {
          return {
            className: worldStyle ? 'mini event' : 'module',
            title: mapText(source.event_kind || 'world event / 世界事件'),
            meta: `${mapText(source.node_name || source.location_id || 'hotspot / 热点')} · ${source.distance_km ?? mapText('global / 全域')} km`,
            code: source.event_id || 'event / 事件',
            focusHtml: mapEventFocusButton(source, 'Track event / 追踪事件'),
          };
        }
        if (kind === 'taskRoute') {
          return {
            className: worldStyle ? 'mini route' : 'module app-avatar-task-route-card',
            title: mapText(source.next_action_label || 'Avatar task route / 角色任务路线'),
            meta: `${mapText(source.from_node_name || source.from_node_id || 'avatar / 角色')} → ${mapText(source.to_node_name || source.to_node_id || 'task node / 任务节点')} · ${mapText(source.latest_status || 'pending / 待推进')}`,
            code: source.task_id || source.route_id || 'avatar_task_route',
            focusHtml: `${mapAvatarTaskRouteFocusButton(source, 'Trace route / 追踪路线')} <span class="hud-chip">${escapeHtml(mapText(source.reward_loop || 'move avatar → complete task → reward / 角色移动 → 完成任务 → 领奖励'))}</span>`,
          };
        }
        if (kind === 'routeRunner') {
          const progress = Math.round(Math.max(0, Math.min(1, Number(source.progress_ratio ?? 0))) * 100);
          const remainingMeters = Math.max(0, Math.round(Number(source.remaining_distance_meters || 0)));
          const etaLabel = mapText(source.eta_label || 'ETA pending / 预计时间待定');
          const traceCount = Array.isArray(source.runner_trace_points) ? source.runner_trace_points.length : 0;
          const checkpoint = source.reward_checkpoint || {};
          const checkpointLabel = mapText(checkpoint.label || source.completion_label || 'Reward checkpoint / 奖励检查点');
          const completionButton = routeRunnerCompletionButtonHtml(source, worldStyle ? 'trillionnium-route-flow-action' : 'trillionnium-app-route-flow-action');
          const historyChips = routeRunnerHistoryChipsHtml(source);
          const partyChips = agentPartyChipsHtml(source);
          return {
            className: worldStyle ? 'mini runner' : 'module app-avatar-route-runner-card',
            title: mapText(source.movement_label || 'Avatar running to task / 角色正在跑向任务'),
            meta: `${mapText(source.from_node_name || source.from_node_id || 'avatar / 角色')} → ${mapText(source.to_node_name || source.to_node_id || 'task node / 任务节点')} · ${progress}% · ${remainingMeters}m · ${etaLabel} · ${checkpointLabel}`,
            code: source.task_id || source.runner_id || 'avatar_route_runner',
            focusHtml: `${mapAvatarTaskRouteFocusButton(source, 'Follow runner / 跟随角色')} ${completionButton} <span class="hud-chip">${escapeHtml(mapText(source.progress_label || `${progress}% route progress / ${progress}% 路线进度`))}</span> <span class="hud-chip">${escapeHtml(etaLabel)}</span> <span class="hud-chip">${traceCount} ${escapeHtml(mapText('trace points / 个追踪点'))}</span> <span class="hud-chip">${escapeHtml(checkpointLabel)}</span> <span class="hud-chip">${escapeHtml(mapText(source.completion_label || 'Complete checkpoint / 完成检查点'))}</span> ${historyChips} ${partyChips}`,
          };
        }
        return {
          className: worldStyle ? 'mini poi' : 'module',
          title: mapText(source.name || 'hotspot / 热点'),
          meta: `${mapText(source.node_kind || 'hotspot / 热点')} · ${source.distance_km ?? 0} km`,
          code: source.node_id || 'node / 节点',
          focusHtml: mapNodeFocusButton(source, 'Focus hotspot / 聚焦热点'),
        };
      };
      const mapViewportCardHtml = (item, kind, style = 'app') => {
        const card = mapViewportCardModel(item, kind, style);
        const focusStack = `<div class="focus-stack">${card.focusHtml}</div>`;
        if (style === 'world') {
          return `<article class="${escapeHtml(card.className)}"><strong>${escapeHtml(card.title)}</strong><span>${escapeHtml(card.meta)}</span><code>${escapeHtml(card.code)}</code>${focusStack}</article>`;
        }
        return `<article class="${escapeHtml(card.className)}"><strong>${escapeHtml(card.title)}</strong><span>${escapeHtml(card.meta)}</span><p><code>${escapeHtml(card.code)}</code></p>${focusStack}</article>`;
      };
      const buildMapFocusFromButton = (button) => {
        const dataset = (button && button.dataset) || {};
        return {
          kind: dataset.focusKind,
          nodeId: dataset.nodeId,
          eventId: dataset.eventId,
          taskId: dataset.taskId,
          locationId: dataset.locationId,
          eventKind: dataset.eventKind,
          nodeName: dataset.nodeName,
          eventBody: dataset.eventBody,
          eventResult: dataset.eventResult,
          suppressAction: dataset.suppressAction === 'true',
          lat: dataset.lat,
          lng: dataset.lng,
          zoom: dataset.zoom,
          z: dataset.tileZ,
          x: dataset.tileX,
          y: dataset.tileY,
        };
      };
      const buildSelectionFocusFromButton = (button) => {
        const dataset = (button && button.dataset) || {};
        const selectionKind = dataset.selectionKind;
        if (selectionKind === 'node') return { kind: 'node', nodeId: dataset.nodeId, suppressAction: true };
        if (selectionKind === 'region') return { kind: 'region', lat: dataset.lat, lng: dataset.lng, zoom: dataset.zoom };
        if (selectionKind === 'tile') return { kind: 'tile', z: dataset.tileZ, x: dataset.tileX, y: dataset.tileY };
        return null;
      };
"#
}

pub(super) fn real_world_map_render_cards_js(style: RealWorldMapShellCardStyle) -> String {
    let style_name = match style {
        RealWorldMapShellCardStyle::AppModule => "app",
        RealWorldMapShellCardStyle::WorldMini => "world",
    };
    format!(
        r#"      const renderCards = (targetNode, items, kind) => {{
        if (!targetNode) return;
        targetNode.innerHTML = items.map((item) => mapViewportCardHtml(item, kind, '{style_name}')).join('');
      }};
"#
    )
}

pub(super) fn real_world_map_route_target_resolution_js() -> &'static str {
    r#"      const resolveRouteTargetNodeId = (locationId, explicitNodeId, fallbackNodeId) => {
        const directNodeId = String(explicitNodeId || '').trim();
        if (directNodeId && markerById.has(directNodeId)) return directNodeId;
        const routeLocationId = String(locationId || '').trim();
        if (routeLocationId) {
          const marker = markerByLocationId.get(routeLocationId);
          if (marker && marker.node_id) return String(marker.node_id);
        }
        return String(fallbackNodeId || '').trim();
      };
"#
}

pub(super) fn real_world_map_route_status_js() -> &'static str {
    r#"      const routeUiLanguage = () => mapLanguage() === 'zh' ? 'zh' : 'en';
      const routePhrase = (en, zh) => routeUiLanguage() === 'zh' ? zh : en;
      const routePlayabilityBody = (body) => {
        const text = String(body || '').trim();
        if (!text) return '';
        const lower = text.toLowerCase();
        const hasCjk = /[\u3400-\u9fff]/.test(text);
        const missingEn = [];
        const missingZh = [];
        if (!(lower.includes('deliver') || lower.includes('customer') || text.includes('客户') || text.includes('交付') || text.includes('方案'))) { missingEn.push('customer deliverable'); missingZh.push('客户交付方案'); }
        if (!(lower.includes('evidence') || lower.includes('source') || lower.includes('data') || text.includes('证据') || text.includes('依据'))) { missingEn.push('evidence package'); missingZh.push('证据包'); }
        if (!(lower.includes('risk') || text.includes('风险'))) { missingEn.push('risk controls'); missingZh.push('风险控制'); }
        if (!(lower.includes('next') || text.includes('下一步') || text.includes('计划'))) { missingEn.push('next action'); missingZh.push('下一步行动'); }
        if (!(lower.includes('review') || lower.includes('self-check') || lower.includes('self check') || text.includes('自评') || text.includes('自检') || text.includes('复盘'))) { missingEn.push('self-review'); missingZh.push('自检复盘'); }
        if (!missingEn.length) return text;
        const endsSentence = /[.!?。！？]$/.test(text);
        if (hasCjk) return text + (endsSentence ? ' ' : '；') + '补齐' + missingZh.join('、') + '。';
        return text + (endsSentence ? ' ' : '; ') + 'add ' + missingEn.join(', ') + '.';
      };
      const routeOpportunitySegment = (task) => {
        const kind = String(((task || {}).next_opportunity_kind) || '').trim();
        return kind ? routePhrase(' · branch ' + mapText(kind), ' · 支线 ' + kind) : '';
      };
      const routeEventBriefText = (eventSignalText, fullRouteVisible) => {
        const normalized = String(eventSignalText || '').replace(/^(Latest event signal:|最新事件信号：)\s*/, '').trim();
        if (!normalized) return routePhrase('Event brief: no live event selected.', '事件简报：尚未选择实时事件。');
        return routePhrase('Event brief: ' + mapText(normalized) + (fullRouteVisible ? ' · full route visible.' : ''), '事件简报：' + normalized + (fullRouteVisible ? ' · 已显示完整路线。' : ''));
      };
      const routeLinkStatusText = (context) => {
        const taskId = String((context || {}).taskId || '').trim();
        if (!taskId) return routePhrase('Linked task route: none yet.', String((context || {}).emptyText || '关联任务路线：暂无。'));
        const linkedEventCount = Number((context || {}).linkedEventCount || 0);
        const linkedContractCount = Number((context || {}).linkedContractCount || 0);
        const contractText = (context || {}).inFocus ? ' 个焦点契约' : ' 个契约';
        const englishContractText = (context || {}).inFocus ? ' focus contracts' : ' contracts';
        return routePhrase('Linked task route: ' + taskId + ' · ' + linkedEventCount + ' events · ' + linkedContractCount + englishContractText + routeOpportunitySegment((context || {}).opportunityTask) + '.', '关联任务路线：' + taskId + ' · ' + linkedEventCount + ' 个事件 · ' + linkedContractCount + contractText + routeOpportunitySegment((context || {}).opportunityTask) + '。');
      };
"#
}

pub(super) fn real_world_map_route_contract_js() -> String {
    let contract_json = world_route_ui_contract_json().to_string();
    format!(
        r#"      const routeUiContract = (() => {{
        const fallback = {contract_json};
        const source = (typeof app !== 'undefined' && app && app.route_contract)
          || (typeof payload !== 'undefined' && payload && payload.route_contract)
          || fallback;
        if (!source || typeof source !== 'object') return fallback;
        return {{
          ...fallback,
          ...source,
          panels: {{ ...(fallback.panels || {{}}), ...((source.panels) || {{}}) }},
          fields: {{ ...(fallback.fields || {{}}), ...((source.fields) || {{}}) }},
          panel_defaults: {{ ...(fallback.panel_defaults || {{}}), ...((source.panel_defaults) || {{}}) }},
          work_lanes: {{ ...(fallback.work_lanes || {{}}), ...((source.work_lanes) || {{}}) }},
          handoff: {{ ...(fallback.handoff || {{}}), ...((source.handoff) || {{}}) }},
        }};
      }})();
      const routeHandoffFields = () => routeUiContract.handoff || {{}};
      const routeHandoffFieldName = (fieldKey, fallback) => routeHandoffFields()[fieldKey] || fallback;
      const routeHandoffStorageKey = () => routeHandoffFieldName('storage_key', 'trillionnium-world-handoff');
      const routeHandoffRead = (handoff, fieldKey, fallback) => {{
        if (!handoff || typeof handoff !== 'object') return fallback;
        const fieldName = routeHandoffFieldName(fieldKey, fallback);
        if (Object.prototype.hasOwnProperty.call(handoff, fieldName) && handoff[fieldName] != null) {{
          return handoff[fieldName];
        }}
        if (fieldKey !== fieldName && Object.prototype.hasOwnProperty.call(handoff, fieldKey) && handoff[fieldKey] != null) {{
          return handoff[fieldKey];
        }}
        return fallback;
      }};
      const routeHandoffValue = (handoff, fieldKey, fallback = '') => {{
        const value = routeHandoffRead(handoff, fieldKey, fallback);
        return value == null ? fallback : value;
      }};
      const routeHandoffPanelId = (handoff, fallback = routeActionPanelId()) => routeHandoffValue(handoff, 'panel_id', fallback);
      const routeHandoffActionLabel = (handoff, fallback = '行动') => {{
        const label = String(routeHandoffValue(handoff, 'action_label', '') || '').trim();
        if (label) return label;
        const actionId = String(routeHandoffValue(handoff, 'action_id', '') || '').trim();
        return actionId || fallback;
      }};
      const routeHandoffCommand = (handoff, fallback = 'prepared') => {{
        const command = String(routeHandoffValue(handoff, 'command', '') || '').trim();
        return command || fallback;
      }};
      const buildRouteHandoffState = (handoff, defaults = {{}}) => {{
        const fallback = defaults || {{}};
        return {{
          nodeId: routeHandoffValue(handoff, 'node_id', fallback.nodeId || ''),
          locationId: routeHandoffValue(handoff, 'location_id', fallback.locationId || ''),
          actionId: routeHandoffValue(handoff, 'action_id', fallback.actionId || ''),
          actionLabel: routeHandoffActionLabel(handoff, fallback.actionLabel || '行动'),
          command: routeHandoffCommand(handoff, fallback.command || 'prepared'),
          panelId: routeHandoffPanelId(handoff, fallback.panelId || routeActionPanelId()),
          actionBody: routeHandoffValue(handoff, 'action_body', fallback.actionBody || ''),
          targetInputId: routeHandoffValue(handoff, 'target_input_id', fallback.targetInputId || ''),
          targetValue: routeHandoffValue(handoff, 'target_value', fallback.targetValue || ''),
          targetTextareaId: routeHandoffValue(handoff, 'target_textarea_id', fallback.targetTextareaId || ''),
          moveTarget: routeHandoffValue(handoff, 'move_target', fallback.moveTarget || ''),
          listingId: routeHandoffValue(handoff, 'listing_id', fallback.listingId || ''),
          workOrderId: routeHandoffValue(handoff, 'work_order_id', fallback.workOrderId || ''),
          contractId: routeHandoffValue(handoff, 'contract_id', fallback.contractId || ''),
          routeTaskId: routeHandoffValue(handoff, 'route_task_id', fallback.routeTaskId || ''),
          eventId: routeHandoffValue(handoff, 'event_id', fallback.eventId || ''),
          eventKind: routeHandoffValue(handoff, 'event_kind', fallback.eventKind || ''),
          eventBody: routeHandoffValue(handoff, 'event_body', fallback.eventBody || ''),
          eventResult: routeHandoffValue(handoff, 'event_result', fallback.eventResult || ''),
        }};
      }};
      const buildMarkerRouteActionState = (action, handoff, nodeId) => buildRouteHandoffState(handoff, {{
        actionLabel: ((action || {{}}).label) || ((action || {{}}).action_id) || '行动',
        command: ((action || {{}}).command) || ('/go ' + nodeId),
        moveTarget: nodeId,
      }});
      const buildRouteHandoffRecord = (payload) => {{
        const source = payload || {{}};
        return {{
          [routeHandoffFieldName('node_id', 'node_id')]: source.node_id || null,
          [routeHandoffFieldName('location_id', 'location_id')]: source.location_id || null,
          [routeHandoffFieldName('action_id', 'action_id')]: source.action_id || null,
          [routeHandoffFieldName('action_label', 'action_label')]: source.action_label || null,
          [routeHandoffFieldName('command', 'command')]: source.command || null,
          [routeHandoffFieldName('panel_id', 'web_panel_id')]: source.web_panel_id || null,
          [routeHandoffFieldName('action_body', 'web_action_body')]: source.web_action_body || null,
          [routeHandoffFieldName('target_input_id', 'web_target_input_id')]: source.web_target_input_id || null,
          [routeHandoffFieldName('target_value', 'web_target_value')]: source.web_target_value || null,
          [routeHandoffFieldName('target_textarea_id', 'web_target_textarea_id')]: source.web_target_textarea_id || null,
          [routeHandoffFieldName('move_target', 'web_move_target')]: source.web_move_target || null,
          [routeHandoffFieldName('listing_id', 'web_listing_id')]: source.web_listing_id || null,
          [routeHandoffFieldName('work_order_id', 'web_work_order_id')]: source.web_work_order_id || null,
          [routeHandoffFieldName('contract_id', 'web_contract_id')]: source.web_contract_id || null,
          [routeHandoffFieldName('route_task_id', 'web_route_task_id')]: source.web_route_task_id || null,
          [routeHandoffFieldName('event_id', 'web_event_id')]: source.web_event_id || null,
          [routeHandoffFieldName('event_kind', 'web_event_kind')]: source.web_event_kind || null,
          [routeHandoffFieldName('event_body', 'web_event_body')]: source.web_event_body || null,
          [routeHandoffFieldName('event_result', 'web_event_result')]: source.web_event_result || null,
        }};
      }};
      const forceRouteFieldValue = (input, value, options = {{}}) => {{
        if (!input || value == null || value === '') return input;
        input.value = value;
        input.dataset.routeAutofilled = 'true';
        if (options.clearManual) input.dataset.routeManual = 'false';
        return input;
      }};
      const forceRouteFieldValueById = (inputId, value, options = {{}}) => {{
        if (!inputId) return null;
        const input = document.getElementById(inputId);
        return forceRouteFieldValue(input, value, options);
      }};
      const scrollRoutePanelIntoView = (panelId) => {{
        const panel = document.getElementById(panelId);
        if (panel) {{
          panel.querySelectorAll('details').forEach((details) => {{ details.open = true; }});
          panel.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
        }}
        return panel;
      }};
"#
    )
}

pub(super) fn real_world_map_route_flow_buttons_js() -> &'static str {
    r#"      const buildRouteActionKey = (action) => {
        return [
          action && action.label || '',
          action && action.panelId || '',
          action && action.inputId || '',
          action && action.value || '',
          action && action.textareaId || '',
          action && action.locationId || '',
          action && action.targetNodeId || '',
          action && action.eventId || '',
          action && action.eventKind || '',
          action && action.eventTaskId || '',
          action && action.workOrderId || '',
          action && action.contractId || '',
          action && action.listingId || '',
          action && action.taskId || '',
          action && action.body || '',
        ].join('|');
      };
      const pushUniqueRouteAction = (items, action) => {
        if (!action || !action.label || !action.panelId || !Array.isArray(items)) return false;
        const key = buildRouteActionKey(action);
        if (items.some((item) => item && item.key === key)) return false;
        items.push({ ...action, key });
        return true;
      };
      const buildRouteOpportunityAction = (task, fallbackLocationId) => {
        if (!task || !(task.next_opportunity_command || task.next_opportunity_body)) return null;
        return buildSimpleRouteTargetAction({
          label: task.next_opportunity_action_label || '推进下一条支线',
          panelId: task.next_opportunity_panel_id || routeActionPanelId(),
          inputId: task.next_opportunity_input_id || '',
          value: task.next_opportunity_input_value || '',
          textareaId: task.next_opportunity_textarea_id || routeActionTextareaId(),
          locationId: task.latest_location_id || fallbackLocationId || '',
          targetNodeId: task.next_opportunity_node_id || '',
          taskId: task.task_id || '',
          status: task.next_opportunity_hint || task.next_opportunity_command || '',
          body: task.next_opportunity_body || task.next_opportunity_command || '',
        });
      };
      const routeFlowActionAttrs = (action) => ` data-target-panel="${escapeHtml(action.panelId)}" data-target-input-id="${escapeHtml(action.inputId || '')}" data-target-value="${escapeHtml(action.value || '')}" data-target-textarea-id="${escapeHtml(action.textareaId || routeActionTextareaId())}" data-target-location-id="${escapeHtml(action.locationId || '')}" data-target-node-id="${escapeHtml(action.targetNodeId || '')}" data-target-task-id="${escapeHtml(action.taskId || '')}" data-target-contract-id="${escapeHtml(action.contractId || '')}" data-target-listing-id="${escapeHtml(action.listingId || '')}" data-target-work-order-id="${escapeHtml(action.workOrderId || '')}" data-target-event-id="${escapeHtml(action.eventId || '')}" data-target-event-kind="${escapeHtml(action.eventKind || '')}" data-target-event-body="${escapeHtml(action.eventBody || '')}" data-target-event-result="${escapeHtml(action.eventResult || '')}" data-target-event-task-id="${escapeHtml(action.eventTaskId || '')}" data-target-body="${escapeHtml(action.body || '')}"`;
      const routeFlowActionButtonHtml = (action, className) => {
        if (!action || !action.label || !action.panelId) return '';
        const buttonClass = String(className || 'trillionnium-route-flow-action').trim() || 'trillionnium-route-flow-action';
        return `<button type="button" class="focus-chip ${escapeHtml(buttonClass)}"${routeFlowActionAttrs(action)}>${escapeHtml(mapText(action.label))}</button>`;
      };
      const pushRouteFlowActionButton = (buttons, keySet, action) => {
        if (!action || !action.label || !action.panelId) return;
        const key = buildRouteActionKey(action);
        if (keySet.has(key)) return;
        keySet.add(key);
        const html = routeFlowActionButtonHtml(action);
        if (html) buttons.push(html);
      };
      const routeTaskGraphActionButtonsHtml = (task, className) => {
        const source = task || {};
        const suggestedAction = buildTaskSuggestedAction(source);
        const opportunityAction = buildRouteOpportunityAction(source, source.latest_location_id || '');
        return routeFlowActionButtonHtml(opportunityAction, className) + routeFlowActionButtonHtml(suggestedAction, className);
      };
      const buildRouteRunnerCompletionAction = (runner) => {
        const source = runner || {};
        const checkpoint = source.reward_checkpoint || {};
        return buildSimpleRouteTargetAction({
          label: source.completion_label || checkpoint.label || 'Complete checkpoint / 完成检查点',
          panelId: routeActionPanelId(),
          textareaId: routeActionTextareaId(),
          locationId: source.latest_location_id || '',
          targetNodeId: source.to_node_id || (checkpoint.node_id || ''),
          taskId: source.task_id || '',
          status: source.completion_status || '',
          body: source.completion_action_body || source.completion_command || source.completion_prompt || '',
        });
      };
      const routeRunnerCompletionButtonHtml = (runner, className = 'trillionnium-route-flow-action') => {
        return routeFlowActionButtonHtml(buildRouteRunnerCompletionAction(runner), className);
      };
      const routeRunnerHistoryChipsHtml = (runner) => {
        const source = runner || {};
        const history = Array.isArray(source.checkpoint_history) ? source.checkpoint_history : [];
        const latestHistory = history.length ? history[history.length - 1] : null;
        const latestLabel = latestHistory ? (latestHistory.label || latestHistory.stage || latestHistory.status || '') : '';
        const chips = [];
        chips.push(`<span class="hud-chip">${escapeHtml(mapText('Checkpoint history / 检查点历史'))}: ${history.length} ${escapeHtml(mapText('history steps / 个历史节点'))}</span>`);
        if (latestLabel) chips.push(`<span class="hud-chip">${escapeHtml(mapText(latestLabel))}</span>`);
        if (source.reward_history_summary) chips.push(`<span class="hud-chip">${escapeHtml(mapText(source.reward_history_summary))}</span>`);
        return chips.join(' ');
      };
      const agentPartyChipsHtml = (source) => {
        const party = Array.isArray((source || {}).agent_party) ? source.agent_party : [];
        const chips = [];
        chips.push(`<span class="hud-chip">${escapeHtml(mapText('Agent party / Agent 小队'))}: ${party.length} ${escapeHtml(mapText('party members / 个小队成员'))}</span>`);
        party.slice(0, 4).forEach((member) => {
          chips.push(`<span class="hud-chip">${escapeHtml(mapText(member.display_name || member.role || 'Agent / Agent'))}</span>`);
        });
        if ((source || {}).agent_party_summary) chips.push(`<span class="hud-chip">${escapeHtml(mapText(source.agent_party_summary))}</span>`);
        return chips.join(' ');
      };
      const indexedRouteActionButtonHtml = (action, index, className = 'trillionnium-app-route-action') => `<button type="button" class="focus-chip ${escapeHtml(className)}" data-route-action-index="${escapeHtml(index)}">${escapeHtml(mapText((action || {}).label || '路线行动'))}</button>`;
"#
}

pub(super) fn real_world_map_route_contract_accessors_js() -> &'static str {
    r#"      const inferConfiguredRouteNextStep = (selection, routeContext, config) => {
        const settings = config || {};
        const selectionTitle = (selection && selection.title) ? selection.title : (settings.selectionTitleFallback || '当前路线');
        const withEventSignal = (body) => appendSelectionEventSignal(body, selection);
        const locationId = routeContext.locationId || '';
        const taskId = routeContext.taskId || '';
        const workOrderId = routeContext.workOrderId || '';
        const contractId = routeContext.contractId || '';
        const listingId = routeContext.listingId || '';
        const latestWorkBucket = routeContext.latestWorkBucket || '';
        const statusPrefix = String(settings.statusPrefix || '推荐下一步');
        const formatStatus = (message) => statusPrefix + ': ' + message;
        const buildAction = (action) => buildSimpleRouteTargetAction({
          locationId,
          taskId,
          workOrderId,
          contractId,
          listingId,
          ...action,
        });
        if (latestWorkBucket === 'rejection' && workOrderId) {
          return buildAction(buildWorldWorkLaneAction('reopen', workOrderId, {
            label: '重开路线',
            body: withEventSignal(settings.rejectionBody(selectionTitle, workOrderId)),
            status: formatStatus(settings.rejectionStatus(workOrderId)),
          }));
        }
        if (latestWorkBucket === 'reopen' && workOrderId) {
          return buildAction(buildWorldWorkLaneAction('delivery', workOrderId, {
            label: '再次提交成果',
            body: withEventSignal(settings.reopenBody(selectionTitle, workOrderId)),
            status: formatStatus(settings.reopenStatus(workOrderId)),
          }));
        }
        if (latestWorkBucket === 'delivery' && workOrderId) {
          return buildAction(buildWorldWorkLaneAction('acceptance', workOrderId, {
            body: withEventSignal(settings.deliveryBody(selectionTitle, workOrderId)),
            status: formatStatus(settings.deliveryStatus(workOrderId)),
          }));
        }
        if ((latestWorkBucket === 'work_order' || latestWorkBucket === 'purchase') && workOrderId) {
          return buildAction(buildWorldWorkLaneAction('delivery', workOrderId, {
            body: withEventSignal(settings.openWorkBody(selectionTitle, workOrderId)),
            status: formatStatus(settings.openWorkStatus(workOrderId)),
          }));
        }
        if ((latestWorkBucket === 'acceptance' || latestWorkBucket === 'cancellation') && workOrderId && settings.closedWorkBody && settings.closedWorkStatus) {
          return buildAction({
            label: settings.closedWorkLabel || '起草后续支线',
            panelId: routeActionPanelId(),
            textareaId: routeActionTextareaId(),
            body: withEventSignal(settings.closedWorkBody(selectionTitle, workOrderId, latestWorkBucket)),
            status: formatStatus(settings.closedWorkStatus(workOrderId, latestWorkBucket)),
          });
        }
        if (contractId) {
          return buildAction(buildWorldContractLaneAction(contractId, {
            body: withEventSignal(settings.contractBody(selectionTitle, contractId)),
            status: formatStatus(settings.contractStatus(contractId)),
          }));
        }
        if (listingId) {
          return buildAction(buildWorldPurchaseLaneAction(listingId, {
            body: withEventSignal(settings.listingBody(selectionTitle, listingId)),
            status: formatStatus(settings.listingStatus(listingId)),
          }));
        }
        return buildAction({
          label: settings.defaultLabel || '起草世界行动',
          panelId: routeActionPanelId(),
          textareaId: routeActionTextareaId(),
          body: withEventSignal(settings.defaultBody(selectionTitle)),
          status: formatStatus(settings.defaultStatus()),
        });
      };
      const buildRouteDraftBody = (selection, routeContext, options) => {
        const settings = options || {};
        const selectionTitle = (selection && selection.title) ? selection.title : (settings.selectionTitleFallback || '当前世界路线');
        const detail = [];
        if (!settings.omitContextDetails) {
          if (routeContext.locationId) detail.push('地点 ' + routeContext.locationId);
          if (routeContext.taskId) detail.push('任务 ' + routeContext.taskId);
          if (routeContext.eventLabel && routeContext.eventLabel !== 'no event') detail.push('事件 ' + routeContext.eventLabel);
          if (routeContext.workOrderId) detail.push('委托 ' + routeContext.workOrderId);
          if (routeContext.contractId) detail.push('契约 ' + routeContext.contractId);
          if (routeContext.listingId) detail.push('任务牌 ' + routeContext.listingId);
          if (routeContext.recommendedLabel) detail.push('下一步 ' + String(routeContext.recommendedLabel).toLowerCase());
        }
        const leadIn = String(settings.leadIn || '：继续推进');
        const detailText = detail.join(' · ') || String(settings.emptyDetail || '当前路线');
        const suffix = String(settings.suffix || '。明确客户交付方案、证据包、评级标准、风险控制、下一步行动和自检复盘。');
        return appendSelectionEventSignal(selectionTitle + leadIn + detailText + suffix, selection);
      };
      const buildTaskFollowUpDraftBody = (selection, taskId, detailText) => {
        return buildRouteDraftBody(selection, {}, {
          selectionTitleFallback: '当前路线',
          omitContextDetails: true,
          leadIn: '：跟进',
          emptyDetail: '任务 ' + taskId + ' ' + detailText,
          suffix: '.',
        });
      };
      const routePanels = () => routeUiContract.panels || {};
      const routeFields = () => routeUiContract.fields || {};
      const routePanelDefaults = (panelId) => ((routeUiContract.panel_defaults || {})[panelId]) || {};
      const routeActionPanelId = () => routePanels().action || 'world-action-console';
      const routeCommercePanelId = () => routePanels().commerce || 'world-commerce-panel';
      const routeContractsPanelId = () => routePanels().contracts || 'world-contracts-panel';
      const routeEventTimelinePanelId = () => routePanels().event_timeline || 'world-event-timeline';
      const routeMapMovePanelId = () => routePanels().map_move || 'world-map-move-panel';
      const routeMoveTargetId = () => routeFields().move_target || 'world-map-move-target';
      const routeActionLocationId = () => routeFields().action_location || 'world-action-location';
      const routePurchaseInputId = () => routeFields().purchase_input || 'world-buy-listing-id';
      const routePurchaseTextareaId = () => routeFields().purchase_textarea || 'world-buy-body';
      const routeContractInputId = () => routeFields().contract_input || 'world-contract-completion-id';
      const routeContractTextareaId = () => routeFields().contract_textarea || 'world-contract-completion-body';
      const routeActionTextareaId = () => routeFields().action_textarea || 'world-action-body';
      const routeWorkLaneKinds = () => {
        const laneOrder = routeUiContract.work_lane_order;
        if (Array.isArray(laneOrder) && laneOrder.length) return laneOrder;
        return ['delivery', 'acceptance', 'rejection', 'reopen', 'cancellation'];
      };
      const routeWorkLaneIds = (laneId) => {
        const normalizedLaneId = String(laneId || '').trim() || 'delivery';
        const workLanes = routeUiContract.work_lanes || {};
        return workLanes[normalizedLaneId] || workLanes.delivery || { inputId: 'world-work-deliver-id', textareaId: 'world-work-deliver-body' };
      };
      const routeWorkLaneInputId = (laneId) => routeWorkLaneIds(laneId).input_id || routeWorkLaneIds(laneId).inputId || '';
      const routeWorkLaneTextareaId = (laneId) => routeWorkLaneIds(laneId).textarea_id || routeWorkLaneIds(laneId).textareaId || '';
      const routePanelInputId = (panelId) => routePanelDefaults(panelId).input_id || routePanelDefaults(panelId).inputId || '';
      const routePanelTextareaId = (panelId) => routePanelDefaults(panelId).textarea_id || routePanelDefaults(panelId).textareaId || routeActionTextareaId();
      const routeWorkLaneInputIds = () => routeWorkLaneKinds().map((laneId) => routeWorkLaneInputId(laneId));
      const routeWorkLaneTextareaIds = () => routeWorkLaneKinds().map((laneId) => routeWorkLaneTextareaId(laneId));
      const routeManualTrackedFieldIds = () => [routePurchaseInputId(), ...routeWorkLaneInputIds(), routeContractInputId(), routePurchaseTextareaId(), ...routeWorkLaneTextareaIds(), routeContractTextareaId(), routeActionTextareaId()];
"#
}

pub(super) fn real_world_map_route_target_builders_js() -> &'static str {
    r#"      const buildTaskSuggestedAction = (task) => {
        const taskInfo = task || {};
        const panelId = taskInfo.suggested_panel_id || routeActionPanelId();
        const contractId = taskInfo.latest_contract_id || '';
        const inputId = taskInfo.suggested_input_id || routePanelInputId(panelId);
        const inputValue = taskInfo.suggested_input_value || (inputId === routeContractInputId() ? contractId : '');
        const textareaId = taskInfo.suggested_textarea_id || routePanelTextareaId(panelId);
        return buildSimpleRouteTargetAction({
          label: taskInfo.suggested_action_label || '起草任务后续',
          panelId,
          inputId,
          value: inputValue,
          textareaId,
          locationId: taskInfo.latest_location_id || '',
          targetNodeId: taskInfo.suggested_node_id || '',
          contractId,
          taskId: taskInfo.task_id || '',
          body: taskInfo.suggested_body || '',
        });
      };
      const buildLinkedContractRouteAction = (selection, contractId, taskId, locationId, options) => {
        const settings = options || {};
        const effectiveContractId = String(contractId || '').trim();
        const effectiveTaskId = String(taskId || effectiveContractId).trim();
        const selectionTitle = (selection && selection.title) ? selection.title : (settings.selectionTitleFallback || '当前路线');
        return buildWorldContractLaneAction(effectiveContractId, {
          label: settings.label || '打开关联契约',
          locationId: locationId || '',
          taskId: effectiveTaskId,
          body: appendSelectionEventSignal(selectionTitle + ': 完成关联契约 ' + effectiveContractId + '，服务任务 ' + effectiveTaskId + (settings.bodySuffix || '。'), selection),
        });
      };
      const buildRouteEventTimelineAction = (label, eventContext) => {
        const event = eventContext || {};
        return {
          label: label || '打开事件时间线',
          panelId: routeEventTimelinePanelId(),
          locationId: String(event.locationId || '').trim(),
          eventId: String(event.eventId || '').trim(),
          eventKind: String(event.eventKind || 'world_event').trim(),
          eventBody: String(event.eventBody || '').trim(),
          eventResult: String(event.eventResult || '').trim(),
          eventTaskId: String(event.eventTaskId || '').trim(),
          body: String(event.body || ''),
        };
      };
      const buildRouteActionFromDataset = (dataset, fallbackLabel) => {
        const source = dataset || {};
        return buildSimpleRouteTargetAction({
          label: String(fallbackLabel || '').trim() || '路线行动',
          panelId: source.targetPanel || routeActionPanelId(),
          inputId: source.targetInputId || '',
          value: source.targetValue || '',
          textareaId: source.targetTextareaId || routeActionTextareaId(),
          locationId: source.targetLocationId || '',
          targetNodeId: source.targetNodeId || '',
          taskId: source.targetTaskId || '',
          contractId: source.targetContractId || '',
          listingId: source.targetListingId || '',
          workOrderId: source.targetWorkOrderId || '',
          eventId: source.targetEventId || '',
          eventKind: source.targetEventKind || '',
          eventBody: source.targetEventBody || '',
          eventResult: source.targetEventResult || '',
          eventTaskId: source.targetEventTaskId || '',
          body: source.targetBody || '',
        });
      };
      const buildRouteActionFromButton = (button, fallbackLabel) => {
        const label = String((button && button.textContent) || '').trim() || fallbackLabel || '路线行动';
        return buildRouteActionFromDataset((button && button.dataset) || {}, label);
      };
      const handleRouteActionButton = (button, opener, fallbackLabel) => {
        if (!button || typeof opener !== 'function') return false;
        opener(buildRouteActionFromButton(button, fallbackLabel));
        return true;
      };
      const handleIndexedRouteActionButton = (button, actions, opener) => {
        if (!button || typeof opener !== 'function') return false;
        const action = (actions || [])[Number(button.dataset.routeActionIndex || -1)];
        if (action) opener(action);
        return true;
      };
      const routeFilterModeFromButton = (button, fallbackMode = 'all') => {
        const value = String(((button && button.dataset) || {}).routeFilter || '').trim();
        if (value === 'all' || value === 'selection') return value;
        return fallbackMode;
      };
      const buildRouteHandoffPayload = (action, selection) => {
        const routeAction = action || {};
        const focus = selection || {};
        const fallbackNodeId = focus.kind === 'node' ? (focus.nodeId || '') : '';
        const targetNodeId = resolveRouteTargetNodeId(routeAction.locationId || focus.locationId || '', routeAction.targetNodeId || '', fallbackNodeId);
        const eventId = String(routeAction.eventId || ((focus.kind === 'event' && focus.eventId) ? focus.eventId : '') || '').trim();
        return {
          targetNodeId,
          eventId,
          payload: buildRouteHandoffRecord({
            node_id: targetNodeId || null,
            location_id: routeAction.locationId || focus.locationId || null,
            action_id: 'app_route_handoff',
            action_label: routeAction.label || '世界路线交接',
            command: '/world',
            web_panel_id: routeAction.panelId || routeActionPanelId(),
            web_action_body: routePlayabilityBody(routeAction.body || ''),
            web_target_input_id: routeAction.inputId || null,
            web_target_value: routeAction.value || null,
            web_target_textarea_id: routeAction.textareaId || null,
            web_move_target: targetNodeId || null,
            web_listing_id: routeAction.listingId || null,
            web_work_order_id: routeAction.workOrderId || null,
            web_contract_id: routeAction.contractId || null,
            web_route_task_id: routeAction.taskId || null,
            web_event_id: eventId || null,
            web_event_kind: routeAction.eventKind || ((focus.kind === 'event' && focus.eventKind) ? focus.eventKind : null),
            web_event_body: routeAction.eventBody || ((focus.kind === 'event' && focus.eventBody) ? focus.eventBody : null),
            web_event_result: routeAction.eventResult || ((focus.kind === 'event' && focus.eventResult) ? focus.eventResult : null),
          }),
        };
      };
      const buildSimpleRouteTargetAction = (config) => {
        const target = config || {};
        return {
          label: target.label || '路线行动',
          panelId: target.panelId || routeActionPanelId(),
          inputId: target.inputId || '',
          value: target.value || '',
          textareaId: target.textareaId || routeActionTextareaId(),
          locationId: target.locationId || '',
          targetNodeId: target.targetNodeId || '',
          workOrderId: target.workOrderId || '',
          contractId: target.contractId || '',
          listingId: target.listingId || '',
          taskId: target.taskId || '',
          eventId: target.eventId || '',
          eventKind: target.eventKind || '',
          eventBody: target.eventBody || '',
          eventResult: target.eventResult || '',
          eventTaskId: target.eventTaskId || '',
          status: target.status || '',
          body: routePlayabilityBody(target.body || ''),
        };
      };
      const buildWorldWorkLaneAction = (laneId, workOrderId, options) => {
        const target = options || {};
        const normalizedLaneId = String(laneId || '').trim();
        const defaultLabel = normalizedLaneId === 'acceptance'
          ? '打开评级路线'
          : normalizedLaneId === 'rejection'
            ? '打开返工路线'
            : normalizedLaneId === 'reopen'
              ? '打开重开路线'
              : normalizedLaneId === 'cancellation'
                ? '打开放弃路线'
                : '打开成果提交路线';
        const effectiveWorkOrderId = String(workOrderId || target.value || target.workOrderId || '').trim();
        return buildSimpleRouteTargetAction({
          ...target,
          label: target.label || defaultLabel,
          panelId: routeCommercePanelId(),
          inputId: routeWorkLaneInputId(normalizedLaneId),
          value: effectiveWorkOrderId,
          textareaId: routeWorkLaneTextareaId(normalizedLaneId),
          workOrderId: target.workOrderId || effectiveWorkOrderId,
        });
      };
      const buildWorldPurchaseLaneAction = (listingId, options) => {
        const target = options || {};
        const effectiveListingId = String(listingId || target.value || target.listingId || '').trim();
        return buildSimpleRouteTargetAction({
          ...target,
          label: target.label || '打开任务牌路线',
          panelId: routeCommercePanelId(),
          inputId: routePurchaseInputId(),
          value: effectiveListingId,
          textareaId: routePurchaseTextareaId(),
          listingId: target.listingId || effectiveListingId,
        });
      };
      const buildWorldContractLaneAction = (contractId, options) => {
        const target = options || {};
        const effectiveContractId = String(contractId || target.value || target.contractId || '').trim();
        return buildSimpleRouteTargetAction({
          ...target,
          label: target.label || '打开契约路线',
          panelId: routeContractsPanelId(),
          inputId: routeContractInputId(),
          value: effectiveContractId,
          textareaId: routeContractTextareaId(),
          contractId: target.contractId || effectiveContractId,
        });
      };
      const buildDraftWorldAction = (locationId, taskId, body) => buildSimpleRouteTargetAction({
        label: '起草世界行动',
        panelId: routeActionPanelId(),
        textareaId: routeActionTextareaId(),
        locationId: locationId || '',
        taskId: taskId || '',
        body: body || '',
      });
      const buildTaskFollowUpAction = (selection, taskId, locationId, detailText) => buildSimpleRouteTargetAction({
        label: '起草任务后续',
        panelId: routeActionPanelId(),
        textareaId: routeActionTextareaId(),
        locationId: locationId || '',
        taskId: taskId || '',
        body: buildTaskFollowUpDraftBody(selection, taskId, detailText),
      });
"#
}

pub(super) fn real_world_map_marker_action_handoff_js() -> &'static str {
    r#"      const resolveMarkerPrimaryAction = (marker, nodeId, actionId) => {
        const source = marker || {};
        const resolvedNodeId = String(source.node_id || nodeId || '').trim();
        return (source.primary_actions || []).find((item) => item.action_id === actionId) || {
          action_id: actionId || 'move_here',
          command: '/go ' + resolvedNodeId,
          label: '移动到这里',
          web_panel_id: routeMapMovePanelId(),
          web_move_target: resolvedNodeId,
        };
      };
      const buildMarkerActionHandoff = (marker, nodeId, actionId) => {
        const source = marker || {};
        const resolvedNodeId = String(source.node_id || nodeId || '').trim();
        const action = resolveMarkerPrimaryAction(source, resolvedNodeId, actionId);
        return {
          marker: source,
          action,
          handoff: buildRouteHandoffRecord({
            node_id: resolvedNodeId || nodeId || null,
            location_id: source.location_id || null,
            action_id: action.action_id || actionId || 'move_here',
            action_label: action.label || action.action_id || '行动',
            command: action.command || ('/go ' + resolvedNodeId),
            web_panel_id: action.web_panel_id || routeActionPanelId(),
            web_action_body: action.web_action_body || '',
            web_move_target: action.web_move_target || resolvedNodeId || null,
          }),
        };
      };
"#
}

pub(super) fn real_world_map_route_action_js() -> String {
    format!(
        "{}{}{}{}",
        real_world_map_route_flow_buttons_js(),
        real_world_map_route_contract_accessors_js(),
        real_world_map_route_target_builders_js(),
        real_world_map_marker_action_handoff_js(),
    )
}

pub(super) fn real_world_map_viewport_hydration_js() -> &'static str {
    r#"      const buildViewportUrl = (mapCenter, zoom) => {
        return viewportTemplate
          .replace('{{lat}}', mapCenter.lat.toFixed(6))
          .replace('{{lng}}', mapCenter.lng.toFixed(6))
          .replace('{{zoom}}', String(zoom))
          .replace('{{radius_km}}', zoom >= 14 ? '4.5' : (zoom >= 10 ? '22.0' : '120.0'))
          .replace('{{limit}}', zoom >= 14 ? '6' : '4');
      };
      const applyViewportSnapshot = (viewport, mapCenter, zoom) => {
        lastViewport = viewport;
        if (densitySummary) {
          densitySummary.textContent = mapText(((viewport.player_density || {}).summary) || '地图密度加载中…');
        }
        if (cameraSummary) {
          cameraSummary.textContent = routePhrase('Camera ' + mapCenter.lat.toFixed(4) + ', ' + mapCenter.lng.toFixed(4) + ' · zoom ' + zoom + ' · ' + mapText(viewport.lod_mode || 'street_nodes') + ' · ' + (viewport.marker_count || 0) + ' visible places · ' + mapText(((viewport.player_density || {}).mode) || 'dense') + ' density · ' + (viewport.live_event_count || 0) + ' live events', '镜头 ' + mapCenter.lat.toFixed(4) + ', ' + mapCenter.lng.toFixed(4) + ' · 缩放 ' + zoom + ' · ' + mapText(viewport.lod_mode || 'street_nodes') + ' · ' + (viewport.marker_count || 0) + ' 个可见地点 · ' + mapText(((viewport.player_density || {}).mode) || 'dense') + ' 密度 · ' + (viewport.live_event_count || 0) + ' 个实时事件');
        }
        renderStreamHud(viewport, lastSelection);
        renderCards(tileTarget, viewport.visible_tile_shards || [], 'tile');
        renderCards(regionTarget, viewport.stream_region_shards || [viewport.active_region || {}], 'region');
        renderCards(poiTarget, viewport.poi_hotspots || [], 'poi');
        renderCards(prefetchTarget, viewport.prefetch_queue || [], 'prefetch');
        renderCards(liveEventTarget, filterLiveEventStream(viewport.live_event_stream || [], lastSelection), 'event');
        renderCards(taskRouteTarget, filterAvatarTaskRoutes(viewport.avatar_task_routes || [], lastSelection), 'taskRoute');
        renderCards(routeRunnerTarget, filterAvatarRouteRunners(viewport.avatar_route_runners || [], lastSelection), 'routeRunner');
        renderViewportOverlays(viewport);
        refreshOverlayControls();
        renderOverlayStatus();
      };
      const fetchViewportSnapshot = async () => {
        const mapCenter = mapAdapter.getCenter(mapRuntime);
        const zoom = mapAdapter.getZoom(mapRuntime);
        const response = await fetch(buildViewportUrl(mapCenter, zoom), { credentials: 'same-origin' });
        if (!response.ok) return null;
        const viewport = await response.json();
        applyViewportSnapshot(viewport, mapCenter, zoom);
        return viewport;
      };
"#
}
