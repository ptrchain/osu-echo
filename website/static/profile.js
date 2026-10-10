/* Local osu! profile interface. AGPL-3.0-or-later. */
'use strict';
const $ = (id) => document.getElementById(id);
const state = {session: null, profile: null, name: null, mode: 'osu', topLimit: 5, recentLimit: 5, request: 0};
const num = (n, digits = 0) => Number(n || 0).toLocaleString('en-US', {minimumFractionDigits: digits, maximumFractionDigits: digits});
const esc = (value) => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function relative(timestamp) {
  if (!timestamp) return 'No recorded plays';
  const seconds = Math.max(0, Date.now() / 1000 - timestamp);
  for (const [unit, size] of [['year',31536000],['month',2592000],['day',86400],['hour',3600],['minute',60]]) {
    if (seconds >= size) { const n = Math.floor(seconds / size); return `${n} ${unit}${n === 1 ? '' : 's'} ago`; }
  }
  return 'just now';
}
async function api(path, body) {
  const options = body === undefined ? {} : {method:'POST',headers:{'Content-Type':'application/json','X-CSRF-Token':state.session.csrf},body:JSON.stringify(body)};
  const res = await fetch(path, options);
  let data;
  try { data = await res.json(); } catch { throw new Error('The local server returned an unexpected response. Restart it and refresh this page.'); }
  if (!res.ok) throw new Error(data.error || 'The request failed.');
  return data;
}
function showMessage(text) { $('page-message').textContent = text; $('page-message').hidden = !text; }
function renderSession() {
  $('account-name').textContent = state.session.user || 'Sign in';
  const v = state.avatarVersion || state.session?.avatar_version || 'default';
  const accountImage = state.session.user ? `/site/avatar?name=${encodeURIComponent(state.session.user)}&v=${encodeURIComponent(v)}` : fallbackAvatar;
  if ($('account-avatar').getAttribute('src') !== accountImage) $('account-avatar').src = accountImage;
  if ($('settings-avatar') && $('settings-avatar').getAttribute('src') !== accountImage) $('settings-avatar').src = accountImage;
  $('account-panel-name').textContent=state.session.user||'Guest';
  $('account-panel-status').textContent=state.session.active===state.session.user?'Active on the local server':'Local osu!';
  $('account-profile').href=state.session.user?`/users/${encodeURIComponent(state.session.user)}${state.mode ? `?mode=${state.mode}` : ''}`:'#profile';
  $('account-settings').hidden=!state.session.user;
  $('account-logout').hidden=!state.session.user;
  $('account-profile').hidden=!state.session.user;
  $('profile-names').replaceChildren(...state.session.profiles.map(name => { const option = document.createElement('option'); option.value = name; return option; }));
  $('signed-in-actions').hidden = !state.session.user;
  $('signed-in-as').textContent = state.session.user ? `Signed in as ${state.session.user}` : '';
}
function openLogin() {
  closeAccount();
  $('login-error').hidden = true;
  $('login-username').value = state.session?.user || state.name || '';
  $('mobile-menu').hidden=true;
  $('mobile-menu-toggle').setAttribute('aria-expanded','false');
  $('login-title').textContent=state.session?.user?'Switch profile':'Sign in';
  $('login-dialog').showModal();
  $('login-username').focus();
  $('login-username').select();
}
function modIcons(value) {
  let mods = String(value || 'NM').match(/.{1,2}/g) || ['NM'];
  mods = [...new Set(mods)].filter(m => !(m==='DT' && mods.includes('NC')) && !(m==='SD' && mods.includes('PF')));
  return `<div class="mods">${mods.map(m=>{const type=['EZ','HT','NF'].includes(m)?'DifficultyReduction':['RX','AP','AT'].includes(m)?'Automation':m==='NM'?'System':'DifficultyIncrease';return `<div class="mod mod--type-${type}" title="${esc(m)}"><div class="mod__icon mod__icon--${esc(m)}" data-acronym="${esc(m)}"></div></div>`;}).join('')}</div>`;
}
function isScorePinned(scoreId) {
  const pinned = (state.profile?.details?.pinned_scores || []).map(String);
  return pinned.includes(String(scoreId));
}
function scoreRow(score, index, top, section = top ? 'top' : 'recent') {
  const map=score.beatmap||{};
  const link=Number(map.beatmap_id)>0?`https://osu.ppy.sh/beatmaps/${Number(map.beatmap_id)}`:null;
  const weight=.95**index;
  const grade=`<div class="score-rank score-rank--full score-rank--${esc(score.grade)}" role="img" aria-label="Grade ${esc(score.grade.replace('X','SS'))}"></div>`;
  const key=`${section}-${index}`;
  const isOwner = Boolean(state.session?.user && state.session.user === state.name);
  const pinned = isScorePinned(score.id);
  const hasReplay = Boolean(score.has_replay);

  return `<div class="play-detail play-detail--highlightable" id="row-${key}"><div class="play-detail__group play-detail__group--top"><div class="play-detail__icon play-detail__icon--main">${grade}</div><div class="play-detail__detail"><a class="play-detail__title u-ellipsis-overflow" ${link?`href="${link}" target="_blank" rel="noreferrer"`:''}>${esc(map.title||'Unknown beatmap')} <small class="play-detail__artist">by ${esc(map.artist||'unknown artist')}</small></a><div class="play-detail__beatmap-and-time"><span class="play-detail__beatmap">${esc(map.version||'Unknown difficulty')}</span><time class="play-detail__time">${relative(score.time)}</time></div></div></div><div class="play-detail__group play-detail__group--bottom"><div class="play-detail__score-detail"><div class="play-detail__icon play-detail__icon--extra">${grade}</div><div class="play-detail__score-detail-top-right"><div class="play-detail__accuracy-and-weighted-pp"><span class="play-detail__accuracy">${num(score.acc,2)}%</span>${top?`<span class="play-detail__weighted-pp">${num(score.pp*weight)}pp</span>`:''}</div>${top?`<div class="play-detail__pp-weight">weighted ${num(weight*100)}%</div>`:''}</div></div><div class="play-detail__mods-pp"><div class="play-detail__mods">${modIcons(score.mods)}</div><div class="play-detail__pp">${num(score.pp)}<span class="play-detail__pp-unit">pp</span></div></div><div class="play-detail__more"><button class="popup-menu" data-score-menu-trigger="${key}" aria-label="Score options for ${esc(map.title||'unknown beatmap')}" aria-expanded="false" type="button"><i class="fas fa-ellipsis-v" aria-hidden="true"></i></button><div class="score-popup-menu" id="menu-${key}" hidden>${isOwner?`<button class="score-popup-item" type="button" data-score-action="pin" data-score-id="${esc(score.id)}">${pinned?'Unpin':'Pin'}</button>`:''}<button class="score-popup-item" type="button" data-score-action="details" data-score-key="${key}">View Details</button>${hasReplay?`<a class="score-popup-item" href="/site/scores/replay?id=${esc(score.id)}" download>Download Replay</a>`:`<button class="score-popup-item is-disabled" type="button" disabled title="No replay data recorded for this score">Download Replay</button>`}${isOwner?`<button class="score-popup-item score-popup-item--remove" type="button" data-score-action="remove" data-score-id="${esc(score.id)}">Remove score</button>`:''}</div></div></div></div><div id="detail-${key}" class="score-expanded" hidden><span>Score <b>${num(score.score)}</b></span><span>Combo <b>${num(score.max_combo)}x</b></span><span>300 / 100 / 50 <b>${num(score.n300)} / ${num(score.n100)} / ${num(score.n50)}</b></span><span>Misses <b>${num(score.nmiss)}</b></span><span>Mods <b>${esc(score.mods)}</b></span>${isOwner?`<button class="subtle-button" data-delete-score="${esc(score.id)}" type="button">Delete score</button>`:''}</div>`;
}
function renderHistory() {
  const history = state.profile.play_history || [];
  if (!history.length) {
    $('play-history').textContent = 'No play history yet.';
    return;
  }
  const values = new Map(history);
  const firstMonthStr = history[0][0];
  const lastMonthStr = history[history.length - 1][0];
  let start = new Date(firstMonthStr + '-01T00:00:00Z');
  if ((values.get(firstMonthStr) || 0) > 0) {
    start.setUTCMonth(start.getUTCMonth() - 1);
  }
  let end = new Date(lastMonthStr + '-01T00:00:00Z');
  const now = new Date();
  const nowMonth = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1));
  if (end < nowMonth) {
    end = nowMonth;
  }

  const months = [];
  const cur = new Date(start.getTime());
  while (cur <= end) {
    const mStr = cur.toISOString().slice(0, 7);
    months.push([mStr, values.get(mStr) || 0]);
    cur.setUTCMonth(cur.getUTCMonth() + 1);
  }
  const data = months.slice(-240);
  const maxVal = Math.max(...data.map(x => x[1]), 1);

  // D3 nice ticks algorithm
  const step0 = maxVal / 3;
  const power = Math.pow(10, Math.floor(Math.log10(step0)));
  const err = step0 / power;
  let step = power;
  if (err >= 7.0710678) step = power * 10;
  else if (err >= 3.1622776) step = power * 5;
  else if (err >= 1.4142135) step = power * 2;

  const ticks = [];
  for (let i = 0; i * step <= maxVal; i++) {
    ticks.push(i * step);
  }
  if (ticks.length < 2) ticks.push(step);
  let topTick = ticks[ticks.length - 1];
  while (maxVal > topTick * 1.15) {
    topTick += step;
    ticks.push(topTick);
  }

  const gridLeft = 96, gridRight = 872;
  const gridWidth = gridRight - gridLeft;
  const gridTop = 20, gridBottom = 195;
  const gridHeight = gridBottom - gridTop;
  const getY = v => gridBottom - (v / topTick) * gridHeight;

  // Horizontal grid lines and Y-axis labels
  const hLines = ticks.map(t => {
    const y = getY(t);
    return `<line x1="${gridLeft}" x2="${gridRight}" y1="${y}" y2="${y}" stroke="rgba(0, 0, 0, 0.4)" stroke-width="1"/><text x="${gridLeft - 10}" y="${y}" dy="0.35em" text-anchor="end" fill="hsl(var(--hsl-f1))" font-size="11" font-family="Torus, Inter, Arial, sans-serif">${num(t)}</text>`;
  }).join('');

  // X-axis ticks (vertical grid lines and rotated labels)
  const monthNames = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const fullMonthNames = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
  const janIndices = [];
  data.forEach(([m], i) => { if (m.endsWith('-01')) janIndices.push(i); });

  let tickIndices = [];
  if (janIndices.length >= 2) {
    tickIndices = janIndices;
  } else if (data.length <= 6) {
    tickIndices = data.map((_, i) => i);
  } else if (data.length <= 18) {
    tickIndices = data.map((_, i) => i).filter(i => i % 2 === 0);
  } else {
    tickIndices = data.map(([m], i) => i).filter(i => {
      const mo = parseInt(data[i][0].slice(5, 7), 10);
      return [1, 4, 7, 10].includes(mo);
    });
  }

  const vLines = tickIndices.map(i => {
    const x = gridLeft + (i / Math.max(data.length - 1, 1)) * gridWidth;
    const mStr = data[i][0];
    const yStr = mStr.slice(0, 4);
    const mIdx = parseInt(mStr.slice(5, 7), 10) - 1;
    const label = `${monthNames[mIdx]} ${yStr}`;
    return `<line x1="${x}" x2="${x}" y1="${gridTop}" y2="${gridBottom}" stroke="rgba(0, 0, 0, 0.4)" stroke-width="1"/><text x="${x}" y="${gridBottom + 10}" transform="rotate(45, ${x}, ${gridBottom + 10})" text-anchor="start" fill="hsl(var(--hsl-f1))" font-size="11" font-family="Torus, Inter, Arial, sans-serif">${label}</text>`;
  }).join('');

  // Data polyline points
  const points = data.map(([m, n], i) => {
    const x = gridLeft + (i / Math.max(data.length - 1, 1)) * gridWidth;
    const y = getY(n);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');

  const html = `<svg id="ph-svg" viewBox="0 0 920 236" role="img" aria-label="Monthly play counts from saved scores">
    ${hLines}
    ${vLines}
    <polyline points="${points}" fill="none" stroke="#ffcc22" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    <g id="ph-hover" opacity="0" pointer-events="none" style="transition: opacity 100ms ease;">
      <line id="ph-hover-line" y1="${gridTop}" y2="${gridBottom}" stroke="rgba(255, 255, 255, 0.25)" stroke-dasharray="3,3" stroke-width="1"/>
      <circle id="ph-hover-dot" r="4.5" fill="#ffcc22" stroke="#251e22" stroke-width="2"/>
      <g id="ph-tooltip">
        <rect id="ph-tt-bg" rx="4" fill="rgba(20, 16, 18, 0.95)" stroke="rgba(255, 255, 255, 0.15)" stroke-width="1" width="130" height="26" x="-65" y="-34"/>
        <text id="ph-tt-text" fill="#fff" font-size="11" font-weight="600" text-anchor="middle" y="-17"></text>
      </g>
    </g>
    <rect id="ph-overlay" x="${gridLeft}" y="0" width="${gridWidth}" height="${gridBottom + 10}" fill="transparent" style="cursor: crosshair;"/>
  </svg>`;

  $('play-history').innerHTML = html;

  const svg = $('ph-svg');
  const overlay = $('ph-overlay');
  const hoverG = $('ph-hover');
  const hoverLine = $('ph-hover-line');
  const hoverDot = $('ph-hover-dot');
  const tt = $('ph-tooltip');
  const ttBg = $('ph-tt-bg');
  const ttText = $('ph-tt-text');

  if (overlay && svg) {
    overlay.addEventListener('mousemove', e => {
      const rect = svg.getBoundingClientRect();
      const svgX = ((e.clientX - rect.left) / rect.width) * 920;
      const ratio = Math.max(0, Math.min(1, (svgX - gridLeft) / gridWidth));
      const idx = Math.round(ratio * (data.length - 1));
      if (idx < 0 || idx >= data.length) return;

      const ptX = gridLeft + (idx / Math.max(data.length - 1, 1)) * gridWidth;
      const ptY = getY(data[idx][1]);
      const [yPart, mPart] = data[idx][0].split('-');
      const mName = fullMonthNames[parseInt(mPart, 10) - 1];
      const textStr = `${mName} ${yPart}: ${num(data[idx][1])} plays`;

      hoverLine.setAttribute('x1', ptX);
      hoverLine.setAttribute('x2', ptX);
      hoverDot.setAttribute('cx', ptX);
      hoverDot.setAttribute('cy', ptY);

      ttText.textContent = textStr;
      const textWidth = Math.max(130, textStr.length * 7 + 16);
      ttBg.setAttribute('width', textWidth);
      ttBg.setAttribute('x', -textWidth / 2);

      let ttY = ptY - 8;
      if (ttY < 30) ttY = ptY + 38;
      let ttX = ptX;
      if (ttX - textWidth / 2 < 10) ttX = textWidth / 2 + 10;
      if (ttX + textWidth / 2 > 910) ttX = 910 - textWidth / 2;

      tt.setAttribute('transform', `translate(${ttX}, ${ttY})`);
      hoverG.setAttribute('opacity', '1');
    });

    overlay.addEventListener('mouseleave', () => {
      hoverG.setAttribute('opacity', '0');
    });
  }
}
function renderMostPlayed() {
  const maps=state.profile.most_played||[];
  $('most-count').textContent=num(maps.length);
  $('most-played').innerHTML=maps.slice(0,state.mostLimit||5).map(({count,beatmap:m})=>{
    const countMarkup=`<span class="beatmap-playcount__count"><i class="fas fa-play beatmap-playcount__count-icon" aria-hidden="true"></i>${num(count)}</span>`;
    const mapId=Number(m.beatmap_id||0);
    const setId=Number(m.beatmapset_id||0);
    const link=mapId>0?`href="https://osu.ppy.sh/beatmaps/${mapId}" target="_blank" rel="noreferrer"`:'';
    const coverTag=link?'a':'div';
    const coverUrl=setId>0?`https://assets.ppy.sh/beatmaps/${setId}/covers/list.jpg`:'';
    const coverUrl2x=setId>0?`https://assets.ppy.sh/beatmaps/${setId}/covers/list@2x.jpg`:'';
    const coverStyle=setId>0?`style="background-image: url('${coverUrl2x}'), url('${coverUrl}');"`:'';
    const coverInner=setId>0?`<span class="beatmapset-cover beatmapset-cover--full" style="--bg: url('${coverUrl}'); --bg-2x: url('${coverUrl2x}'); background-image: url('${coverUrl2x}');"></span>`:'';
    return `<div class="beatmap-playcount"><${coverTag} class="beatmap-playcount__cover" ${link} ${coverStyle}>${coverInner}<div class="beatmap-playcount__cover-count">${countMarkup}</div></${coverTag}><div class="beatmap-playcount__detail"><div class="beatmap-playcount__info"><div class="u-ellipsis-overflow"><a class="beatmap-playcount__title" ${link}>${esc(m.title||'Unknown beatmap')} <span class="beatmap-playcount__title-artist">by ${esc(m.artist||'unknown artist')}</span></a></div><div class="beatmap-playcount__info-row u-ellipsis-overflow"><span class="beatmap-playcount__artist">${esc(m.artist||'unknown artist')}</span><span>${esc(m.version||'?')}</span>${m.creator?` <span class="beatmap-playcount__mapper">mapped by <b>${esc(m.creator)}</b></span>`:''}</div></div><div class="beatmap-playcount__detail-count">${countMarkup}</div></div></div>`;
  }).join('')||'<p class="empty-inline">No plays recorded yet.</p>';
  $('more-most').hidden=maps.length<=(state.mostLimit||5);
}
function renderScores() {
  const p = state.profile;
  const pinned = p.pinned || [];
  if ($('pinned-section')) {
    $('pinned-section').hidden = pinned.length === 0;
    if ($('pinned-count')) $('pinned-count').textContent = num(pinned.length);
    if ($('pinned-scores')) {
      $('pinned-scores').innerHTML = pinned.length ? pinned.map((s, i) => scoreRow(s, i, false, 'pinned')).join('') : '';
    }
  }
  $('top-count').textContent = num(p.top.length);
  const recent=p.recent_24h||[];
  $('recent-count').textContent = num(recent.length);
  $('top-scores').innerHTML = p.top.length ? p.top.slice(0,state.topLimit).map((s,i)=>scoreRow(s,i,true,'top')).join('') : '<p class="empty-inline">No ranked plays yet. Your best performances will appear here.</p>';
  $('recent-scores').innerHTML = recent.length ? recent.slice(0,state.recentLimit).map((s,i)=>scoreRow(s,i,false,'recent')).join('') : '<p class="empty-inline">No plays in the last 24 hours.</p>';
  $('more-top').hidden = p.top.length <= state.topLimit;
  $('more-recent').hidden = recent.length <= state.recentLimit;
}

async function loadMedalsCatalog() {
  if (state.medalsCatalog) return state.medalsCatalog;
  try {
    const res = await fetch('/site/medals');
    if (res.ok) {
      state.medalsCatalog = await res.json();
      return state.medalsCatalog;
    }
  } catch (e) {
    console.error('Failed to load medals catalog', e);
  }
  return [];
}

let medalTooltipEl = null;
function setupMedalTooltips() {
  if (!medalTooltipEl) {
    medalTooltipEl = document.createElement('div');
    medalTooltipEl.id = 'medal-tooltip';
    medalTooltipEl.className = 'medal-tooltip';
    medalTooltipEl.setAttribute('role', 'tooltip');
    medalTooltipEl.innerHTML = `
      <div id="medal-tooltip-category" class="medal-tooltip__header"></div>
      <div class="medal-tooltip__body">
        <div class="medal-tooltip__icon-wrapper">
          <img id="medal-tooltip-img" class="medal-tooltip__img" src="" alt="" />
        </div>
        <div id="medal-tooltip-name" class="medal-tooltip__name"></div>
        <div id="medal-tooltip-desc" class="medal-tooltip__desc"></div>
        <div class="medal-tooltip__footer">
          <div id="medal-tooltip-status" class="medal-tooltip__status"></div>
          <div id="medal-tooltip-date" class="medal-tooltip__date"></div>
        </div>
      </div>
      <div class="medal-tooltip__arrow"></div>
    `;
    document.body.appendChild(medalTooltipEl);
  }

  const container = $('medals-content');
  if (!container || container._tooltipsInitialized) return;
  container._tooltipsInitialized = true;

  const showTip = (badge) => {
    if (!badge || !badge.classList.contains('badge-achievement')) return;
    const name = badge.dataset.name;
    const desc = badge.dataset.desc;
    const icon = badge.dataset.icon;
    const category = badge.dataset.category || 'Medals';
    const isUnlocked = badge.dataset.unlocked === 'true';
    const achieved = badge.dataset.achieved;

    $('medal-tooltip-category').textContent = category;

    const imgEl = $('medal-tooltip-img');
    imgEl.src = `https://assets.ppy.sh/medals/client/${icon}@2x.png`;
    imgEl.onerror = () => {
      imgEl.src = `https://assets.ppy.sh/medals/web/${icon}.png`;
    };
    imgEl.className = 'medal-tooltip__img ' + (isUnlocked ? 'medal-tooltip__img--unlocked' : 'medal-tooltip__img--locked');

    $('medal-tooltip-name').textContent = name;
    $('medal-tooltip-desc').textContent = desc;

    const statusEl = $('medal-tooltip-status');
    const dateEl = $('medal-tooltip-date');
    if (isUnlocked) {
      statusEl.textContent = 'Unlocked';
      statusEl.className = 'medal-tooltip__status medal-tooltip__status--unlocked';
      if (achieved) {
        dateEl.textContent = `Achieved ${achieved}`;
        dateEl.style.display = 'block';
      } else {
        dateEl.textContent = '';
        dateEl.style.display = 'none';
      }
    } else {
      statusEl.textContent = 'Locked';
      statusEl.className = 'medal-tooltip__status medal-tooltip__status--locked';
      dateEl.textContent = '';
      dateEl.style.display = 'none';
    }

    medalTooltipEl.classList.add('visible');

    const rect = badge.getBoundingClientRect();
    const tipW = medalTooltipEl.offsetWidth || 240;
    const tipH = medalTooltipEl.offsetHeight || 290;
    let left = rect.left + rect.width / 2 - tipW / 2;
    left = Math.max(12, Math.min(window.innerWidth - tipW - 12, left));

    let isAbove = true;
    let top = rect.top - tipH - 12;
    if (top < 12) {
      top = rect.bottom + 12;
      isAbove = false;
    }
    medalTooltipEl.classList.toggle('arrow-down', isAbove);
    medalTooltipEl.classList.toggle('arrow-up', !isAbove);

    const arrowEl = medalTooltipEl.querySelector('.medal-tooltip__arrow');
    if (arrowEl) {
      const badgeCenterX = rect.left + rect.width / 2;
      const arrowX = Math.max(18, Math.min(tipW - 18, badgeCenterX - left));
      arrowEl.style.left = `${arrowX}px`;
    }

    medalTooltipEl.style.left = `${left + window.scrollX}px`;
    medalTooltipEl.style.top = `${top + window.scrollY}px`;
  };

  const hideTip = () => {
    if (medalTooltipEl) medalTooltipEl.classList.remove('visible');
  };

  container.addEventListener('mouseover', (e) => {
    const badge = e.target.closest('.badge-achievement');
    if (badge) showTip(badge);
  });
  container.addEventListener('mouseout', (e) => {
    const badge = e.target.closest('.badge-achievement');
    if (badge) hideTip();
  });
  container.addEventListener('focusin', (e) => {
    const badge = e.target.closest('.badge-achievement');
    if (badge) showTip(badge);
  });
  container.addEventListener('focusout', (e) => {
    const badge = e.target.closest('.badge-achievement');
    if (badge) hideTip();
  });
}

async function renderMedals() {
  const container = $('medals-content');
  if (!container) return;

  const catalog = await loadMedalsCatalog();
  if (!catalog || !catalog.length) {
    container.innerHTML = '<p class="empty-inline">Unable to load medals catalog.</p>';
    return;
  }

  const userMedals = state.profile?.medals || [];
  const userMedalsMap = new Map();
  for (const m of userMedals) {
    userMedalsMap.set(m.id, m);
  }

  if ($('medals-count-badge')) {
    $('medals-count-badge').textContent = `${num(userMedals.length)} / ${num(catalog.length)}`;
  }

  // Find up to 8 most recently achieved medals
  const recentUserAchievements = [];
  const sortedUserMedals = [...userMedals].sort((a, b) => (b.achieved_at || 0) - (a.achieved_at || 0));
  for (const um of sortedUserMedals) {
    const fullMedal = catalog.find(c => c.id === um.id) || um;
    recentUserAchievements.push({
      ...fullMedal,
      achieved_at: um.achieved_at,
    });
    if (recentUserAchievements.length >= 8) break;
  }

  let html = '';

  // 1. Render Latest medals box if user has earned medals
  if (recentUserAchievements.length > 0) {
    html += `<div class="page-extra__recent-medals-box">`;
    html += `  <h3 class="title title--page-extra-small">Latest</h3>`;
    html += `  <div class="page-extra__recent-medals">`;
    for (const m of recentUserAchievements) {
      const dateStr = m.achieved_at 
        ? new Date(m.achieved_at * 1000).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' }) 
        : '';
      html += `<div class="badge-achievement badge-achievement--dynamic-height badge-achievement--unlocked" tabindex="0" role="button"`;
      html += ` data-id="${m.id}"`;
      html += ` data-name="${esc(m.name)}"`;
      html += ` data-desc="${esc(m.description)}"`;
      html += ` data-category="${esc(m.category)}"`;
      html += ` data-icon="${esc(m.icon_url)}"`;
      html += ` data-unlocked="true"`;
      html += ` data-achieved="${esc(dateStr)}"`;
      html += ` aria-label="${esc(m.name)}">`;
      html += `  <img class="badge-achievement__image"`;
      html += `       src="https://assets.ppy.sh/medals/client/${esc(m.icon_url)}@2x.png"`;
      html += `       alt="${esc(m.name)}" loading="lazy"`;
      html += `       onerror="this.onerror=null;this.src='https://assets.ppy.sh/medals/web/${esc(m.icon_url)}.png';">`;
      html += `</div>`;
    }
    html += `  </div>`;
    html += `</div>`;
  }

  // 2. Official osu!web section order
  const sections = [
    { id: 'beatmap-challenge-packs', title: 'Beatmap Challenge Packs', isHush: false, filter: m => m.category === 'Beatmap Packs' && m.ordering === 5 },
    { id: 'beatmap-packs', title: 'Beatmap Packs', isHush: false, filter: m => m.category === 'Beatmap Packs' && (m.ordering === 6 || m.ordering === 8) },
    { id: 'beatmap-spotlights', title: 'Beatmap Spotlights', isHush: false, filter: m => m.category === 'Beatmap Packs' && m.ordering === 7 },
    { id: 'hush-hush', title: 'Hush-Hush', isHush: true, filter: m => m.category === 'Hush-Hush' && m.ordering === 2 },
    { id: 'hush-hush-expert', title: 'Hush-Hush (Expert)', isHush: true, filter: m => m.category === 'Hush-Hush' && m.ordering === 3 },
    { id: 'skill-dedication', title: 'Skill & Dedication', isHush: false, filter: m => m.category === 'Skill & Dedication' },
    { id: 'mod-introduction', title: 'Mod Introduction', isHush: false, filter: m => m.category === 'Mod Introduction' },
  ];

  for (const sec of sections) {
    const items = catalog.filter(sec.filter);
    if (!items.length) continue;

    html += `<div class="medals-group__group" id="medals-sec-${sec.id}">`;
    html += `  <h3 class="medals-group__title">${esc(sec.title)}</h3>`;
    html += `  <div class="medals-grid">`;
    for (const m of items) {
      const userMedal = userMedalsMap.get(m.id);
      const isUnlocked = !!userMedal;
      const dateStr = userMedal && userMedal.achieved_at 
        ? new Date(userMedal.achieved_at * 1000).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' }) 
        : '';

      const displayName = (!isUnlocked && sec.isHush) ? '???' : m.name;
      const displayDesc = (!isUnlocked && sec.isHush) ? '???' : m.description;
      const stateCls = isUnlocked ? 'badge-achievement--unlocked' : 'badge-achievement--locked';

      html += `<div class="badge-achievement ${stateCls}" tabindex="0" role="button"`;
      html += ` data-id="${m.id}"`;
      html += ` data-name="${esc(displayName)}"`;
      html += ` data-desc="${esc(displayDesc)}"`;
      html += ` data-category="${esc(m.category)}"`;
      html += ` data-icon="${esc(m.icon_url)}"`;
      html += ` data-unlocked="${isUnlocked}"`;
      html += ` data-achieved="${esc(dateStr)}"`;
      html += ` aria-label="${esc(displayName)}">`;
      html += `  <img class="badge-achievement__image"`;
      html += `       src="https://assets.ppy.sh/medals/client/${esc(m.icon_url)}@2x.png"`;
      html += `       alt="${esc(displayName)}" loading="lazy"`;
      html += `       onerror="this.onerror=null;this.src='https://assets.ppy.sh/medals/web/${esc(m.icon_url)}.png';">`;
      html += `</div>`;
    }
    html += `  </div>`;
    html += `</div>`;
  }

  container.innerHTML = html;
  setupMedalTooltips();
}
// Stable level thresholds: osu! rounds each score increment individually.
// https://osu.ppy.sh/wiki/en/Gameplay/Score/Total_score
function scoreLevel(total) {
  const formula=n=>5000/3*(4*n**3-3*n**2-n)+1.25*1.8**(n-60);
  let level=1, lower=0, upper=0;
  if(total>=26931190827){
    level=100+Math.floor((total-26931190827)/99999999999);
    lower=26931190827+(level-100)*99999999999;upper=lower+99999999999;
  }else{
    while(level<100){upper=lower+Math.round(formula(level+1)-formula(level));if(total<upper)break;lower=upper;level++;}
  }
  return {level,progress:Math.max(0,Math.min(100,Math.floor((total-lower)/(upper-lower)*100)))};
}
const countryNames=new Intl.DisplayNames(['en'],{type:'region'});
const countryName=code=>{
  if(!code||typeof code!=='string')return '';
  const clean=code.trim().toUpperCase();
  try{return countryNames.of(clean)||clean;}catch(_){return clean;}
};
function countryFlagHex(code){
  if(!code||typeof code!=='string')return '';
  const clean=code.trim().toUpperCase();
  if(clean.length!==2)return '';
  return [...clean].map(c=>(127397+c.charCodeAt(0)).toString(16)).join('-');
}
function countryFlagBackground(code){
  const hex=countryFlagHex(code);
  if(!hex)return 'none';
  return `url('https://osu.ppy.sh/assets/images/flags/${hex}.svg'),url('https://cdn.jsdelivr.net/gh/twitter/twemoji@14.0.2/assets/svg/${hex}.svg')`;
}
function parseBBCode(raw) {
  if (!raw) return '';
  let s = esc(raw);
  s = s.replace(/\[b\]([\s\S]*?)\[\/b\]/gi, '<strong>$1</strong>');
  s = s.replace(/\[i\]([\s\S]*?)\[\/i\]/gi, '<em>$1</em>');
  s = s.replace(/\[u\]([\s\S]*?)\[\/u\]/gi, '<u>$1</u>');
  s = s.replace(/\[(?:s|strike)\]([\s\S]*?)\[\/(?:s|strike)\]/gi, '<del>$1</del>');
  s = s.replace(/\[heading\]([\s\S]*?)\[\/heading\]/gi, '<h3 class="bbcode-heading">$1</h3>');
  s = s.replace(/\[(?:centre|center)\]([\s\S]*?)\[\/(?:centre|center)\]/gi, '<div class="bbcode-centre">$1</div>');
  s = s.replace(/\[color=([#\w]+)\]([\s\S]*?)\[\/color\]/gi, (m, c, t) => /^[#a-zA-Z0-9]+$/.test(c) ? `<span style="color:${c}">${t}</span>` : t);
  s = s.replace(/\[size=(\d+)\]([\s\S]*?)\[\/size\]/gi, (m, sz, t) => `<span style="font-size:${Math.min(Math.max(Number(sz)||100,50),300)}%">${t}</span>`);
  s = s.replace(/\[url=(https?:\/\/[^\s"'>]+)\]([\s\S]*?)\[\/url\]/gi, '<a href="$1" target="_blank" rel="noreferrer" class="bbcode-link">$2</a>');
  s = s.replace(/\[url\](https?:\/\/[^\s"'>]+)\[\/url\]/gi, '<a href="$1" target="_blank" rel="noreferrer" class="bbcode-link">$1</a>');
  s = s.replace(/\[img\](https?:\/\/[^\s"'>]+)\[\/img\]/gi, '<img src="$1" alt="image" class="bbcode-image" loading="lazy">');
  s = s.replace(/\[box=([^\]]+)\]([\s\S]*?)\[\/box\]/gi, '<details class="bbcode-box"><summary class="bbcode-box__header">$1</summary><div class="bbcode-box__content">$2</div></details>');
  s = s.replace(/\[box\]([\s\S]*?)\[\/box\]/gi, '<details class="bbcode-box"><summary class="bbcode-box__header">Show details</summary><div class="bbcode-box__content">$1</div></details>');
  s = s.replace(/\[spoiler\]([\s\S]*?)\[\/spoiler\]/gi, '<span class="bbcode-spoiler" title="Spoiler">$1</span>');
  s = s.replace(/\[quote=([^\]]+)\]([\s\S]*?)\[\/quote\]/gi, '<blockquote class="bbcode-quote"><div class="bbcode-quote__author">$1 wrote:</div><div class="bbcode-quote__content">$2</div></blockquote>');
  s = s.replace(/\[quote\]([\s\S]*?)\[\/quote\]/gi, '<blockquote class="bbcode-quote"><div class="bbcode-quote__content">$1</div></blockquote>');
  s = s.replace(/\[list\]([\s\S]*?)\[\/list\]/gi, (m, items) => {
    const listItems = items.split(/\[\*\]/).filter(x => x.trim().length > 0).map(x => `<li>${x.trim()}</li>`).join('');
    return `<ul class="bbcode-list">${listItems}</ul>`;
  });
  s = s.replace(/\n/g, '<br>');
  s = s.replace(/<br>\s*(<\/?(?:div|details|summary|blockquote|h3|ul|li))/gi, '$1');
  s = s.replace(/(<\/(?:div|details|summary|blockquote|h3|ul|li)>)\s*<br>/gi, '$1');
  return s;
}

function renderProfile() {
  const p = state.profile;
  document.title = `${p.name} · player info · Local osu!`;
  $('username').textContent = p.name;
  const details=p.details||{};
  for(const id of details.section_order||defaultSectionOrder){
    const section=$(id),link=document.querySelector(`.section-tabs a[href="#${id}"]`);
    if(section&&link){document.querySelector('.user-profile-pages').append(section);document.querySelector('.section-tabs').append(link);}
  }
  const countryCode=(details.country||'').trim().toUpperCase();
  const countryEl=$('profile-country');
  if(countryCode&&countryCode.length===2){
    const cName=countryName(countryCode)||countryCode;
    countryEl.href=`https://osu.ppy.sh/rankings/osu/performance?country=${encodeURIComponent(countryCode)}`;
    countryEl.title=cName;
    countryEl.setAttribute('aria-label',cName);
    countryEl.innerHTML=`<span class="flag-country flag-country--medium" style="background-image:${countryFlagBackground(countryCode)}" aria-hidden="true"></span><span class="profile-info__flag-text">${esc(cName)}</span>`;
    countryEl.removeAttribute('hidden');
    countryEl.style.display='';
  }else{
    countryEl.removeAttribute('href');
    countryEl.removeAttribute('title');
    countryEl.replaceChildren();
    countryEl.style.display='none';
  }
  const linksRow = $('profile-links-row');
  if (linksRow) {
    linksRow.replaceChildren();

    // 1. Join date
    const joinDateVal = details.join_date || p.first_play;
    let joinDateText = '';
    if (joinDateVal) {
      const d = typeof joinDateVal === 'number' ? new Date(joinDateVal * 1000) : new Date(joinDateVal);
      if (!isNaN(d.getTime())) {
        joinDateText = d.toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
      }
    }
    if (!joinDateText) {
      joinDateText = new Date().toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
    }
    const joinItem = document.createElement('span');
    joinItem.className = 'profile-links__item';
    joinItem.innerHTML = `Joined <span class="profile-links__value">${esc(joinDateText)}</span>`;
    linksRow.appendChild(joinItem);

    // 2. Online / Last seen status
    const statusItem = document.createElement('span');
    statusItem.className = 'profile-links__item';
    if (p.active) {
      statusItem.innerHTML = `<span class="profile-links__value">Currently online</span>`;
    } else if (p.last_play) {
      statusItem.innerHTML = `Last seen <span class="profile-links__value">${relative(p.last_play)}</span>`;
    } else {
      statusItem.innerHTML = `Last seen <span class="profile-links__value">recently</span>`;
    }
    linksRow.appendChild(statusItem);

    // 3. Playstyle / devices
    if (details.devices && details.devices.length) {
      const devItem = document.createElement('span');
      devItem.className = 'profile-links__item';
      devItem.innerHTML = `Plays with <span class="profile-links__value">${esc(details.devices.join(', '))}</span>`;
      linksRow.appendChild(devItem);
    }

    // 4. Location with map-marker icon
    if (details.location && details.location.trim()) {
      const locItem = document.createElement('span');
      locItem.className = 'profile-links__item';
      locItem.innerHTML = `<span class="profile-links__icon"><i class="fas fa-map-marker-alt" aria-hidden="true"></i></span><span class="profile-links__value">${esc(details.location.trim())}</span>`;
      linksRow.appendChild(locItem);
    }

    // 5. Interests
    if (details.interests && details.interests.trim()) {
      const intItem = document.createElement('span');
      intItem.className = 'profile-links__item';
      intItem.innerHTML = `<span class="profile-links__icon"><i class="fas fa-heart" aria-hidden="true"></i></span><span class="profile-links__value">${esc(details.interests.trim())}</span>`;
      linksRow.appendChild(intItem);
    }

    // 6. Occupation
    if (details.occupation && details.occupation.trim()) {
      const occItem = document.createElement('span');
      occItem.className = 'profile-links__item';
      occItem.innerHTML = `<span class="profile-links__icon"><i class="fas fa-briefcase" aria-hidden="true"></i></span><span class="profile-links__value">${esc(details.occupation.trim())}</span>`;
      linksRow.appendChild(occItem);
    }

    // 7. Twitter
    if (details.twitter && details.twitter.trim()) {
      const twHandle = details.twitter.trim().replace(/^@/, '');
      const twItem = document.createElement('a');
      twItem.className = 'profile-links__item';
      twItem.href = `https://twitter.com/${encodeURIComponent(twHandle)}`;
      twItem.target = '_blank';
      twItem.rel = 'noreferrer';
      twItem.innerHTML = `<span class="profile-links__icon"><i class="fab fa-twitter" aria-hidden="true"></i></span><span class="profile-links__value">@${esc(twHandle)}</span>`;
      linksRow.appendChild(twItem);
    }

    // 8. Discord
    if (details.discord && details.discord.trim()) {
      const discItem = document.createElement('span');
      discItem.className = 'profile-links__item';
      discItem.innerHTML = `<span class="profile-links__icon"><i class="fab fa-discord" aria-hidden="true"></i></span><span class="profile-links__value">${esc(details.discord.trim())}</span>`;
      linksRow.appendChild(discItem);
    }

    // 9. Website
    if (details.website && details.website.trim()) {
      const rawUrl = details.website.trim();
      const href = /^https?:\/\//i.test(rawUrl) ? rawUrl : `https://${rawUrl}`;
      const dispUrl = rawUrl.replace(/^https?:\/\//i, '').replace(/\/$/, '');
      const webItem = document.createElement('a');
      webItem.className = 'profile-links__item';
      webItem.href = href;
      webItem.target = '_blank';
      webItem.rel = 'noreferrer';
      webItem.innerHTML = `<span class="profile-links__icon"><i class="fas fa-globe" aria-hidden="true"></i></span><span class="profile-links__value">${esc(dispUrl)}</span>`;
      linksRow.appendChild(webItem);
    }

    // 10. Forum posts (if any)
    if (details.post_count != null && details.post_count > 0) {
      const postItem = document.createElement('span');
      postItem.className = 'profile-links__item';
      postItem.innerHTML = `Contributed <span class="profile-links__value">${num(details.post_count)} forum ${details.post_count === 1 ? 'post' : 'posts'}</span>`;
      linksRow.appendChild(postItem);
    }

    // 11. Comments (if any)
    if (details.comments_count != null && details.comments_count > 0) {
      const commItem = document.createElement('span');
      commItem.className = 'profile-links__item';
      commItem.innerHTML = `Posted <span class="profile-links__value">${num(details.comments_count)} ${details.comments_count === 1 ? 'comment' : 'comments'}</span>`;
      linksRow.appendChild(commItem);
    }
  }
  if(details.about){
    $('about-content').innerHTML=parseBBCode(details.about);
    $('about-content').classList.remove('subdued');
  }else{
    $('about-content').textContent="This player hasn't written anything about themselves yet.";
    $('about-content').classList.add('subdued');
  }
  const v = (state.session?.user === p.name && state.avatarVersion) ? state.avatarVersion : (p.avatar_version || state.avatarVersion || 'default');
  $('avatar').src = `/site/avatar?name=${encodeURIComponent(p.name)}&v=${encodeURIComponent(v)}`;
  $('edit-profile').hidden=state.session.user!==p.name;
  $('edit-profile-details').hidden=state.session.user!==p.name;
  if ($('edit-about')) $('edit-about').hidden = true;
  const level=scoreLevel(p.total_score);
  $('level-number').textContent=level.level;
  $('level-badge').title=`Level ${level.level} · calculated from saved total score`;
  $('level-fill').style.width=`${level.progress}%`;
  $('level-percent').textContent=`${level.progress}%`;
  $('level-progress').setAttribute('aria-valuenow',level.progress);
  $('avatar').alt = `${p.name}'s avatar`;
  $('presence').textContent = p.active ? 'Active' : '';
  $('presence').classList.toggle('active', p.active);
  if(p.global_rank){
    const highest=getHighestRank(p);
    const highestHtml=highest?`<div class="rank-highest-popup">Highest rank: #${num(highest.rank)} on ${formatHighestDate(highest.updated_at)}</div>`:'';
    $('global-rank').innerHTML=`#${num(p.global_rank)}${highestHtml}`;
    $('global-rank').removeAttribute('title');
  }else{
    $('global-rank').textContent='—';
    $('global-rank').title='osu!daily rank unavailable. Check the configured API key or try again later.';
  }
  if($('country-rank')){
    if(p.country_rank){
      $('country-rank').textContent=`#${num(p.country_rank)}`;
      $('country-rank').removeAttribute('title');
    }else{
      $('country-rank').textContent='—';
      $('country-rank').title='Country rank unavailable. Requires valid osu! API OAuth credentials, configured country, and ranked plays.';
    }
  }
  $('pp').textContent = num(p.pp);
  const stats = [['Ranked Score',num(p.ranked_score)],['Hit Accuracy',`${num(p.acc,2)}%`],['Play Count',num(p.playcount)],['Total Score',num(p.total_score)],['Total Hits',num(p.total_hits)],['Hits per Play',num(p.playcount ? Math.floor(p.total_hits/p.playcount) : 0)],['Maximum Combo',`${num(p.max_combo)}x`],['Replays Watched by Others','—']];
  $('statistics').innerHTML = stats.map(([key,value])=>`<dl class="profile-stats__entry"><dt class="profile-stats__key">${key}</dt><dd class="profile-stats__value">${value}</dd></dl>`).join('');
  const gradeOrder = ['XH', 'X', 'SH', 'S', 'A'];
  const gradeLabels = { XH: 'SSH', X: 'SS', SH: 'SH', S: 'S', A: 'A' };
  $('grade-counts').innerHTML = gradeOrder.map(grade => {
    const label = gradeLabels[grade];
    const count = (p.grades && p.grades[grade] != null) ? p.grades[grade] : ((p.grades && p.grades[label] != null) ? p.grades[label] : 0);
    return `<div class="profile-rank-count__item"><div class="profile-rank-count__rank"><div class="score-rank score-rank--${grade} score-rank--rank-${label.toLowerCase()}" role="img" aria-label="${label}"></div></div><span>${num(count)}</span></div>`;
  }).join('');
  $('activity').innerHTML = p.recent.length ? p.recent.slice(0,5).map(s=>`<div class="activity-row"><span class="score-rank score-rank--tiny score-rank--${esc(s.grade)}" aria-label="Grade ${esc(s.grade)}"></span><div><b>${esc(p.name)}</b> played <a ${Number(s.beatmap.beatmap_id)>0?`href="https://osu.ppy.sh/beatmaps/${Number(s.beatmap.beatmap_id)}" target="_blank" rel="noreferrer"`:''}>${esc(s.beatmap.title||'an unknown beatmap')} [${esc(s.beatmap.version||'?')}]</a> with <b>${num(s.pp)}pp</b></div><time>${relative(s.time)}</time></div>`).join('') : '<p class="empty-inline">No recent activity.</p>';
  const medalsCount = (p.medals && p.medals.length) || 0;
  if ($('medals-count')) $('medals-count').textContent = num(medalsCount);
  if ($('medals-count-badge')) $('medals-count-badge').textContent = num(medalsCount);
  renderMedals();
  renderPerformanceGraph();
  requestAnimationFrame(updateSection);
  renderScores(); renderHistory(); renderMostPlayed();
  updateModeSelector();
  $('welcome').hidden = true;
  $('profile-content').hidden = false;
}
async function loadProfile(name, navigate = false, quiet = false) {
  if (!name) { $('welcome').hidden = false; $('profile-content').hidden = true; return; }
  const request = ++state.request;
  if (!quiet) showMessage('Loading profile…');
  try {
    const profile = await api(`/site/profile?name=${encodeURIComponent(name)}&mode=${state.mode}`);
    if (request !== state.request) return;
    state.profile = profile; state.name = name;
    if (navigate) history.pushState({},'',`/users/${encodeURIComponent(name)}?mode=${state.mode}`);
    renderProfile(); showMessage('');
  } catch (error) {
    if (request !== state.request) return;
    if (!quiet) { $('profile-content').hidden = true; showMessage(error.message); }
  }
}
function isSettingsUrl(path = location.pathname, hash = location.hash) {
  const p = (path || '').toLowerCase();
  const h = (hash || '').toLowerCase();
  return p === '/settings' || p === '/settings/' || p === '/home/account/edit' || p === '/home/account/edit/' || p === '/osu/settings' || p === '/osu/settings/' || h === '#settings' || h === '#account-settings' || h === '#import-scores' || h.startsWith('#settings-sec-');
}
async function showSettingsPage(sectionId = null, navigate = false) {
  if (!state.session?.user) {
    openLogin();
    return;
  }
  closeAccount();
  document.body.classList.add('t-settings');
  document.body.classList.remove('t-user');
  if ($('welcome')) $('welcome').hidden = true;
  if ($('profile-content')) $('profile-content').hidden = true;
  const shell = document.querySelector('.profile-shell');
  if (shell) shell.hidden = true;
  const nav = document.querySelector('.profile-navigation');
  if (nav) nav.hidden = true;
  const heading = document.querySelector('.page-heading');
  if (heading) heading.hidden = true;
  if ($('settings-page')) $('settings-page').hidden = false;
  document.title = 'account settings · Local osu!';

  document.querySelectorAll('.dashboard-subnav__item, .header-nav-v4__link').forEach(item => {
    item.classList.remove('is-active', 'header-nav-v4__link--active');
  });
  const activeTab = (sectionId === 'settings-sec-scores') ? $('dash-link-import') : $('dash-link-account');
  if (activeTab) {
    activeTab.classList.add('is-active', 'header-nav-v4__link--active');
  }

  const coverBg = $('settings-cover-bg');
  if (coverBg) {
    const bgUrl = state.profile?.cover_url || '/site/static/vendor/generic@2x.ef7ea3b5.jpg';
    coverBg.style.backgroundImage = `url('${bgUrl}')`;
  }

  if (navigate) {
    const targetUrl = '/settings' + (sectionId ? '#' + sectionId : '');
    if (location.pathname !== '/settings' || location.hash !== (sectionId ? '#' + sectionId : '')) {
      history.pushState({ page: 'settings', section: sectionId }, '', targetUrl);
    }
  }

  await loadSettingsData();

  if (sectionId) {
    const el = $(sectionId);
    if (el) {
      setTimeout(() => el.scrollIntoView({ behavior: 'smooth', block: 'start' }), 60);
    }
  } else {
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }
}
async function showProfilePage(name = null, navigate = false) {
  document.body.classList.remove('t-settings');
  document.body.classList.add('t-user');
  if ($('settings-page')) $('settings-page').hidden = true;
  const shell = document.querySelector('.profile-shell');
  if (shell) shell.hidden = false;
  const nav = document.querySelector('.profile-navigation');
  if (nav) nav.hidden = false;
  const heading = document.querySelector('.page-heading');
  if (heading) heading.hidden = false;
  const targetName = name || state.name || state.session?.user || state.session?.active || state.session?.default;
  await loadProfile(targetName, navigate);
}
async function boot() {
  try {
    state.session = await api('/site/session');
    renderSession();
    const query = new URLSearchParams(location.search);
    state.mode = ['osu','taiko','fruits','mania','vn','rx','ap'].includes(query.get('mode')) ? query.get('mode') : 'osu';
    if (state.mode === 'vn') state.mode = 'osu';
    updateModeSelector();

    if (isSettingsUrl()) {
      const hash = location.hash.replace('#', '');
      const secId = hash.startsWith('settings-sec-') ? hash : (hash === 'scores' || hash === 'import-scores' ? 'settings-sec-scores' : null);
      if (!state.session?.user) {
        openLogin();
        return;
      }
      await showSettingsPage(secId, false);
      return;
    }

    let name = /\/(?:users|u)\/([^/]+)/.exec(location.pathname)?.[1];
    if (name) name = decodeURIComponent(name);
    // The game's fixed user ID 2 links to its current local profile.
    if (name === '2' && !state.session.profiles.includes('2')) name = state.session.active;
    await showProfilePage(name || state.session.user || state.session.active || state.session.default, false);
  } catch (error) { showMessage(`Cannot load the local server: ${error.message}`); }
}
function closeAccount(){ $('account-panel').hidden=true;$('account').setAttribute('aria-expanded','false'); }
$('account').addEventListener('click',()=>{
  if(!state.session?.user){openLogin();return;}
  $('account-panel').hidden=!$('account-panel').hidden;
  $('account').setAttribute('aria-expanded',String(!$('account-panel').hidden));
});
$('account-switch').addEventListener('click',openLogin);
$('account-settings').addEventListener('click',()=>{closeAccount();showSettingsPage('settings-sec-profile',true);});
$('account-logout').addEventListener('click',()=>{closeAccount();$('logout').click();});
document.addEventListener('click',event=>{if(!event.target.closest('#account-panel,#account'))closeAccount();});
document.addEventListener('keydown',event=>{if(event.key==='Escape'&&!$('account-panel').hidden){closeAccount();$('account').focus();}});
$('mobile-account').addEventListener('click',openLogin);
$('mobile-menu-toggle').addEventListener('click',()=>{const hidden=!$('mobile-menu').hidden;$('mobile-menu').hidden=hidden;$('mobile-menu-toggle').setAttribute('aria-expanded',String(!hidden));});
$('first-login').addEventListener('click',openLogin);
function dismissLoginDialog(){
  $('login-dialog').close();
  if (isSettingsUrl() && !state.session?.user) {
    showProfilePage(state.session?.default || 'mrekk', true);
  }
}
$('close-login').addEventListener('click', dismissLoginDialog);
$('login-dialog').addEventListener('click',event=>{
  if(event.target===$('login-dialog')){
    const r=$('login-dialog').getBoundingClientRect();
    if(event.clientX<r.left||event.clientX>r.right||event.clientY<r.top||event.clientY>r.bottom) dismissLoginDialog();
  }
});
$('login-form').addEventListener('submit',async event=>{
  event.preventDefault(); $('login-error').hidden=true; $('login-submit').disabled=true;
  try {
    if(!state.session) state.session=await api('/site/session');
    const result=await api('/site/login',{username:$('login-username').value});
    state.session=await api('/site/session'); renderSession();
    state.topLimit=5;state.recentLimit=5;
    if (isSettingsUrl()) {
      await showSettingsPage(null, false);
    } else {
      await loadProfile(result.name,true);
    }
    $('login-dialog').close();
  } catch(error){$('login-error').textContent=error.message;$('login-error').hidden=false;}
  finally{$('login-submit').disabled=false;}
});
$('logout').addEventListener('click',async()=>{
  try{
    await api('/site/logout',{});
    state.session=await api('/site/session');
    renderSession();
    if (isSettingsUrl()) {
      await showProfilePage(state.session.default, true);
    } else {
      renderProfile();
    }
    $('login-dialog').close();
  }
  catch(error){$('login-error').textContent=error.message;$('login-error').hidden=false;}
});
function updateModeSelector(){
  const currentRuleset = ['osu', 'vn', 'rx', 'ap'].includes(state.mode) ? 'osu' : (state.mode || 'osu');

  document.querySelectorAll('#game-mode-nav .game-mode-link').forEach(btn => {
    const btnMode = btn.dataset.mode;
    const isActive = btnMode === currentRuleset;
    btn.classList.toggle('game-mode-link--active', isActive);
    btn.setAttribute('aria-pressed', String(isActive));

    // Handle badges on osu button for relax and autopilot
    if (btnMode === 'osu') {
      let badge = btn.querySelector('.game-mode-link__badge');
      if (state.mode === 'rx') {
        if (!badge) {
          badge = document.createElement('span');
          badge.className = 'game-mode-link__badge';
          btn.appendChild(badge);
        }
        badge.textContent = 'rx';
        btn.title = 'osu! (Relax)';
        btn.setAttribute('aria-label', 'osu! (Relax)');
      } else if (state.mode === 'ap') {
        if (!badge) {
          badge = document.createElement('span');
          badge.className = 'game-mode-link__badge';
          btn.appendChild(badge);
        }
        badge.textContent = 'ap';
        btn.title = 'osu! (Autopilot)';
        btn.setAttribute('aria-label', 'osu! (Autopilot)');
      } else {
        if (badge) badge.remove();
        btn.title = 'osu!';
        btn.setAttribute('aria-label', 'osu!');
      }
    }
  });
}
document.querySelectorAll('#game-mode-nav .game-mode-link').forEach(btn => {
  btn.addEventListener('click', async () => {
    const clickedMode = btn.dataset.mode;
    const currentRuleset = ['osu', 'vn', 'rx', 'ap'].includes(state.mode) ? 'osu' : state.mode;

    let targetMode = clickedMode;
    if (clickedMode === 'osu') {
      if (currentRuleset === 'osu') {
        // VN button is already selected! Cycle osu -> rx -> ap -> osu
        if (state.mode === 'osu' || state.mode === 'vn') {
          targetMode = 'rx';
        } else if (state.mode === 'rx') {
          targetMode = 'ap';
        } else {
          targetMode = 'osu';
        }
      } else {
        targetMode = 'osu';
      }
    } else {
      if (state.mode === clickedMode) return;
      targetMode = clickedMode;
    }

    state.mode = targetMode;
    updateModeSelector();
    state.topLimit = 5; state.recentLimit = 5; state.mostLimit = 5;
    await loadProfile(state.name, true);
  });
});
$('more-top').addEventListener('click',()=>{state.topLimit+=10;renderScores();});
$('more-recent').addEventListener('click',()=>{state.recentLimit+=10;renderScores();});
const defaultSectionOrder=['me','top-ranks','historical','beatmaps','medals','recent-activity'];
let sectionFrame=0;
function updateSection(){
  sectionFrame=0;
  const sectionLinks=[...document.querySelectorAll('.section-tabs a')];
  if(!sectionLinks.length) return;
  const offset=matchMedia('(max-width:899px)').matches?105:65;
  let current=sectionLinks[0];
  for(const link of sectionLinks){
    try {
      const target = link.hash ? document.querySelector(link.hash) : null;
      if(target && target.getBoundingClientRect().top <= offset) current = link;
    } catch (_) {}
  }
  for(const link of sectionLinks){link.classList.toggle('selected',link===current);if(link===current)link.setAttribute('aria-current','location');else link.removeAttribute('aria-current');}
}
window.addEventListener('scroll',()=>{if(!sectionFrame)sectionFrame=requestAnimationFrame(updateSection);},{passive:true});
window.addEventListener('resize',updateSection);
window.addEventListener('popstate',boot);
// Keep new scores visible without changing the browser's selected profile.
setInterval(async()=>{
  if(document.hidden||$('login-dialog').open||document.querySelector('.score-expanded:not([hidden])')||document.querySelector('.score-popup-menu:not([hidden])')||($('settings-page')&&!$('settings-page').hidden)||!state.name)return;
  try{state.session=await api('/site/session');renderSession();await loadProfile(state.name,false,true);}catch{}
},30000);
boot();

const fallbackAvatar='/site/static/vendor/avatar-guest@2x.01495bc4.png';
$('account-avatar').addEventListener('error',()=>{if(!$('account-avatar').src.endsWith(fallbackAvatar))$('account-avatar').src=fallbackAvatar;});
$('avatar').addEventListener('error',()=>{if(!$('avatar').src.endsWith(fallbackAvatar))$('avatar').src=fallbackAvatar;});
$('settings-avatar').addEventListener('error',()=>{if(!$('settings-avatar').src.endsWith(fallbackAvatar))$('settings-avatar').src=fallbackAvatar;});
$('more-most').addEventListener('click',()=>{state.mostLimit=(state.mostLimit||5)+10;renderMostPlayed();});
function closeAllScoreMenus() {
  document.querySelectorAll('.score-popup-menu:not([hidden])').forEach(m => {
    m.hidden = true;
  });
  document.querySelectorAll('.play-detail.play-detail--menu-active').forEach(row => {
    row.classList.remove('play-detail--menu-active');
  });
  document.querySelectorAll('[data-score-menu-trigger][aria-expanded="true"]').forEach(btn => {
    btn.setAttribute('aria-expanded', 'false');
  });
}

document.addEventListener('click', async event => {
  const trigger = event.target.closest('[data-score-menu-trigger]');
  if (trigger) {
    const key = trigger.dataset.scoreMenuTrigger;
    const menu = $('menu-' + key);
    const row = $('row-' + key);
    if (!menu) return;
    const wasOpen = !menu.hidden;
    closeAllScoreMenus();
    if (!wasOpen) {
      menu.hidden = false;
      if (row) row.classList.add('play-detail--menu-active');
      trigger.setAttribute('aria-expanded', 'true');
    }
    return;
  }

  const actionBtn = event.target.closest('[data-score-action]');
  if (actionBtn) {
    const action = actionBtn.dataset.scoreAction;
    const scoreId = actionBtn.dataset.scoreId;
    const key = actionBtn.dataset.scoreKey;
    closeAllScoreMenus();

    if (action === 'details') {
      const detail = $('detail-' + key);
      if (detail) {
        detail.hidden = !detail.hidden;
      }
    } else if (action === 'pin') {
      try {
        await api('/site/scores/pin', { id: String(scoreId) });
        await loadProfile(state.name);
      } catch (err) {
        showMessage(err.message);
      }
    } else if (action === 'remove') {
      if (!confirm('Delete this score from your profile? Your pp and statistics will be recalculated. A recovery copy is retained on the server.')) return;
      try {
        await api('/site/scores/delete', { id: String(scoreId) });
        await loadProfile(state.name);
      } catch (err) {
        showMessage(err.message);
      }
    }
    return;
  }

  if (event.target.closest('.score-popup-menu a')) {
    closeAllScoreMenus();
    return;
  }

  if (!event.target.closest('.play-detail__more')) {
    closeAllScoreMenus();
  }
});

document.addEventListener('keydown', event => {
  if (event.key === 'Escape') {
    closeAllScoreMenus();
  }
});
function initCountryList() {
  const select = $('settings-country');
  if (!select) return;
  if (select.options.length <= 1) {
    const codes = "AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS YE YT ZA ZM ZW".split(' ');
    codes.sort((a,b) => countryName(a).localeCompare(countryName(b))).forEach(code => {
      const opt = document.createElement('option');
      opt.value = code;
      opt.textContent = countryName(code);
      select.append(opt);
    });
    select.addEventListener('change', updateSettingsCountryFlag);
  }
}
initCountryList();

function updateSettingsCountryFlag(){
  const flagEl=$('settings-country-flag');
  if(!flagEl)return;
  const val=($('settings-country')?.value||'').trim().toUpperCase();
  if(val&&val.length===2){
    flagEl.style.backgroundImage=countryFlagBackground(val);
    flagEl.style.display='inline-block';
  }else{
    flagEl.style.display='none';
  }
}
function flashStatus(pillId, text = 'Updated!'){
  const pill=$(pillId);
  if(!pill)return;
  pill.textContent=text;
  pill.hidden=false;
  clearTimeout(pill._timeout);
  pill._timeout=setTimeout(()=>{ pill.hidden=true; },2500);
}
function showSettingsStatus(msg, type = 'info'){
  const banner=$('settings-status-banner');
  if(!banner)return;
  banner.textContent=msg;
  banner.className='settings-status-banner '+(type==='error'?'settings-status-banner--error':'settings-status-banner--success');
  banner.hidden=!msg;
  if(msg&&type!=='error'){
    clearTimeout(banner._timeout);
    banner._timeout=setTimeout(()=>{ banner.hidden=true; },3500);
  }
}
async function loadSettingsData(){
  if(!state.session?.user)return;
  state.avatarUpload=null;
  state.resetAvatar=false;
  if($('avatar-file')) $('avatar-file').value='';
  if($('settings-status-banner')) $('settings-status-banner').hidden=true;
  if($('settings-username-display')) $('settings-username-display').textContent=state.session.user;
  if($('settings-username')) $('settings-username').value=state.session.user;
  if($('settings-username-view')) $('settings-username-view').hidden=false;
  if($('settings-username-edit')) $('settings-username-edit').hidden=true;

  const v=state.avatarVersion||state.session?.avatar_version||Date.now();
  if($('settings-avatar')) $('settings-avatar').src=`/site/avatar?name=${encodeURIComponent(state.session.user)}&v=${encodeURIComponent(v)}`;

  if($('import-scores-query') && !$('import-scores-query').value) $('import-scores-query').value=state.session.user;

  try{
    const profile=await api(`/site/profile?name=${encodeURIComponent(state.session.user)}&mode=${state.mode||'osu'}`);
    const details=profile.details||{};
    if($('settings-country')) $('settings-country').value=details.country||'';
    updateSettingsCountryFlag();
    if($('settings-location')) $('settings-location').value=details.location||'';
    if($('settings-interests')) $('settings-interests').value=details.interests||'';
    if($('settings-occupation')) $('settings-occupation').value=details.occupation||'';
    if($('settings-twitter')) $('settings-twitter').value=details.twitter||'';
    if($('settings-discord')) $('settings-discord').value=details.discord||'';
    if($('settings-website')) $('settings-website').value=details.website||'';
    if($('settings-about')) $('settings-about').value=details.about||'';
    updateSignaturePreview();

    state.sectionOrder=[...(details.section_order||defaultSectionOrder)];
    renderSectionOrder();

    const devices=details.devices||[];
    document.querySelectorAll('input[name=device]').forEach(input=>{
      input.checked=devices.includes(input.value);
    });

    const currentMode=['osu','taiko','fruits','mania'].includes(state.mode)?state.mode:'osu';
    const modeRadio=document.querySelector(`input[name="playmode"][value="${currentMode}"]`);
    if(modeRadio)modeRadio.checked=true;

    ['settings-location', 'settings-interests', 'settings-occupation', 'settings-twitter', 'settings-discord', 'settings-website'].forEach(id => {
      const el = $(id);
      if (el) el._lastSavedValue = el.value;
    });
    if ($('settings-country')) $('settings-country')._lastSavedValue = $('settings-country').value;
  }catch(error){
    showSettingsStatus(error.message,'error');
  }
}
function openSettings(tab = 'profile'){
  showSettingsPage(tab==='scores'?'settings-sec-scores':'settings-sec-profile',true);
}
function selectSettingsTab(tab){
  showSettingsPage(tab==='scores'?'settings-sec-scores':'settings-sec-profile',true);
}
function showImportStatus(msg,type){
  const el=$('import-official-status');
  el.textContent=msg;el.className='import-status '+(type||'');el.hidden=!msg;
}
function setImportProgressBar(percent, current, total, importedCount, replaysCount, stage, error = false){
  const bar = $('import-progress-bar-fill');
  const counter = $('import-progress-counter');
  const pctEl = $('import-progress-percent');
  if(!bar) return;

  bar.className = 'import-progress-bar-fill' + (error ? ' error' : (percent >= 100 ? ' success' : ''));
  bar.style.width = Math.min(100, Math.max(0, percent)) + '%';
  if(pctEl) pctEl.textContent = Math.round(percent) + '%';

  if(counter){
    if(total > 0){
      let text = `${current} / ${total} scores`;
      if(importedCount !== undefined && importedCount > 0){
        text += ` · ${importedCount} saved`;
        if(replaysCount) text += `, ${replaysCount} replays`;
      }
      counter.textContent = text;
    }else if(stage){
      counter.textContent = stage;
    }else{
      counter.textContent = '';
    }
  }
}

function showImportScoresStatus(msg, type, percent = 0){
  const el = $('import-scores-status');
  el.textContent = msg;
  el.className = 'import-status ' + (type || '');
  $('import-scores-progress-box').hidden = !msg;
  const spinner = $('import-scores-spinner');
  if(spinner) spinner.style.display = type === 'loading' ? 'block' : 'none';

  if(type === 'loading'){
    setImportProgressBar(percent, 0, 0, 0, 0, 'Starting...');
  }else if(type === 'success'){
    setImportProgressBar(100, 0, 0, undefined, undefined, 'Complete');
  }else if(type === 'error'){
    setImportProgressBar(100, 0, 0, undefined, undefined, 'Failed', true);
  }
}

document.querySelectorAll('#import-mode-toggles .mode-toggle-btn').forEach(btn=>{
  btn.addEventListener('click',()=>{
    btn.classList.toggle('active');
    if(!document.querySelectorAll('#import-mode-toggles .mode-toggle-btn.active').length){
      btn.classList.add('active');
    }
  });
});
$('import-official-btn').addEventListener('click',async()=>{
  const query=$('import-official-query').value.trim();
  if(!query){showImportStatus('Enter an official osu! username or user ID.','error');$('import-official-query').focus();return;}
  const btn=$('import-official-btn');
  btn.disabled=true;showImportStatus('Fetching profile from osu.ppy.sh...','loading');
  try{
    const res=await api('/site/import_official',{query,apply:false,import_avatar:true,import_bio:true,import_details:true});
    const details=res.details||{};
    if(details.about!==undefined)$('settings-about').value=details.about;
    if(details.location!==undefined)$('settings-location').value=details.location;
    if(details.country!==undefined){$('settings-country').value=details.country;updateSettingsCountryFlag();}
    if(details.interests!==undefined)$('settings-interests').value=details.interests;
    if(details.occupation!==undefined)$('settings-occupation').value=details.occupation;
    if(details.twitter!==undefined)$('settings-twitter').value=details.twitter;
    if(details.discord!==undefined)$('settings-discord').value=details.discord;
    if(details.website!==undefined)$('settings-website').value=details.website;
    if(Array.isArray(details.devices)){
      document.querySelectorAll('input[name=device]').forEach(input=>{input.checked=details.devices.includes(input.value);});
    }
    if(res.avatar_data_url){
      $('settings-avatar').src=res.avatar_data_url;
      state.avatarUpload=res.avatar_data_url;
      state.resetAvatar=false;
    }
    showImportStatus(`Imported ${res.official_username} (#${res.official_id})! Review and click update in each section to save.`,'success');
  }catch(err){showImportStatus(err.message,'error');}
  finally{btn.disabled=false;}
});
$('import-official-query').addEventListener('keydown',e=>{
  if(e.key==='Enter'){e.preventDefault();$('import-official-btn').click();}
});
if($('import-scores-query')){
  $('import-scores-query').addEventListener('keydown',e=>{
    if(e.key==='Enter'){e.preventDefault();$('import-scores-btn').click();}
  });
}
$('import-scores-btn').addEventListener('click',async()=>{
  const query=$('import-scores-query').value.trim();
  if(!query){
    showImportScoresStatus('Enter an official osu! username or user ID.','error');
    $('import-scores-query').focus();
    return;
  }
  const modes=[...document.querySelectorAll('#import-mode-toggles .mode-toggle-btn.active')].map(b=>b.dataset.mode);
  if(!modes.length)modes.push('osu');
  const types=[...document.querySelectorAll('input[name="import-type"]:checked')].map(cb=>cb.value);
  if(!types.length){
    showImportScoresStatus('Select at least one score category (e.g. Top Ranks).','error');
    return;
  }
  const conflict_policy=$('import-scores-policy').value||'replace_better';
  const download_replays=$('import-scores-replays').checked;
  const sync_playcount=$('import-scores-playcount').checked;
  const sessionVal=$('import-scores-session').value.trim();
  const osu_session=sessionVal?sessionVal:undefined;

  const btn=$('import-scores-btn');
  btn.disabled=true;
  const panel=$('settings-sec-scores')||$('settings-scores-panel');
  const controls=panel ? panel.querySelectorAll('input, button, select') : [];
  controls.forEach(c=>{if(c!==btn)c.disabled=true;});

  showImportScoresStatus(`Connecting to osu.ppy.sh for '${query}'...`,'loading',5);

  let pollInterval=null;
  const startPolling=()=>{
    pollInterval=setInterval(async()=>{
      try{
        const p=await api('/site/scores/import/status');
        if(p&&p.active){
          if(p.message)$('import-scores-status').textContent=p.message;
          setImportProgressBar(
            p.percent||0,
            p.current||0,
            p.total||0,
            p.imported_count||0,
            p.replays_count||0,
            p.stage
          );
        }
      }catch(_){}
    },250);
  };

  try{
    startPolling();
    const res=await api('/site/scores/import',{
      query,
      modes,
      types,
      conflict_policy,
      download_replays,
      sync_playcount,
      osu_session,
    });
    clearInterval(pollInterval);
    setImportProgressBar(100,res.imported_count,res.imported_count,res.imported_count,res.replays_downloaded,'Complete');
    const medalPart=res.medals_unlocked>0?` • ${res.medals_unlocked} new medal(s) unlocked!`:'';
    const successMsg=`Successfully imported ${res.imported_count} score(s) (${res.replays_downloaded} replays)! PP: ${num(res.new_pp)}pp | Acc: ${num(res.new_acc,2)}%${medalPart}`;
    showImportScoresStatus(successMsg,'success');
    if(state.name){
      await loadProfile(state.name,false,true);
    }
  }catch(err){
    clearInterval(pollInterval);
    showImportScoresStatus(err.message,'error');
  }finally{
    clearInterval(pollInterval);
    btn.disabled=false;
    controls.forEach(c=>{c.disabled=false;});
  }
});
$('import-scores-query').addEventListener('keydown',e=>{
  if(e.key==='Enter'){e.preventDefault();$('import-scores-btn').click();}
});
// Settings update & submission logic
async function saveSettingsSection(patch, pillId, successMsg = 'Updated!'){
  if (!state.session?.user) return;
  if ($('settings-status-banner')) $('settings-status-banner').hidden = true;
  try {
    const curDetails = (state.profile && state.profile.name === state.session.user) ? (state.profile.details || {}) : {};
    const newDetails = {
      ...curDetails,
      country: $('settings-country')?.value || '',
      location: $('settings-location')?.value || '',
      interests: $('settings-interests')?.value || '',
      occupation: $('settings-occupation')?.value || '',
      twitter: $('settings-twitter')?.value || '',
      discord: $('settings-discord')?.value || '',
      website: $('settings-website')?.value || '',
      about: $('settings-about') ? $('settings-about').value : (curDetails.about || ''),
      devices: [...document.querySelectorAll('input[name=device]:checked')].map(i => i.value),
      section_order: state.sectionOrder || defaultSectionOrder,
      ...patch,
    };

    const body = {
      username: state.session.user,
      reset_avatar: Boolean(state.resetAvatar),
      details: newDetails,
    };
    if (state.avatarUpload) {
      body.avatar = state.avatarUpload;
    }

    const res = await api('/site/settings', body);
    state.session = await api('/site/session');
    renderSession();
    if (state.name === state.session.user) {
      await loadProfile(state.session.user, false, true);
    }
    if (pillId) flashStatus(pillId, successMsg);
    return res;
  } catch (err) {
    showSettingsStatus(err.message, 'error');
    throw err;
  }
}

const on = (id, event, handler) => { const el = $(id); if (el) el.addEventListener(event, handler); };

// Rename profile modal handlers
function openRenameDialog() {
  const dlg = $('rename-dialog');
  if (!dlg) return;
  const currentName = state.session?.user || state.name || '';
  if ($('rename-current-user')) $('rename-current-user').textContent = currentName;
  if ($('settings-username')) $('settings-username').value = currentName;
  if ($('rename-error')) $('rename-error').hidden = true;
  dlg.showModal();
  if ($('settings-username')) {
    setTimeout(() => {
      $('settings-username').focus();
      $('settings-username').select();
    }, 50);
  }
}

function closeRenameDialog() {
  const dlg = $('rename-dialog');
  if (dlg && dlg.open) dlg.close();
}

on('btn-toggle-rename', 'click', openRenameDialog);
on('close-rename', 'click', closeRenameDialog);
on('btn-cancel-rename', 'click', closeRenameDialog);

const renameDlg = $('rename-dialog');
if (renameDlg) {
  renameDlg.addEventListener('click', event => {
    if (event.target === renameDlg) {
      const r = renameDlg.getBoundingClientRect();
      if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) {
        closeRenameDialog();
      }
    }
  });
}

const renameForm = $('rename-form');
if (renameForm) {
  renameForm.addEventListener('submit', async event => {
    event.preventDefault();
    const newName = ($('settings-username')?.value || '').trim();
    const errEl = $('rename-error');
    if (errEl) errEl.hidden = true;

    if (!newName) {
      if (errEl) { errEl.textContent = 'Please enter a username.'; errEl.hidden = false; }
      return;
    }
    if (newName === state.session?.user) {
      closeRenameDialog();
      return;
    }

    const btn = $('btn-submit-rename');
    if (btn) btn.disabled = true;

    try {
      const curDetails = (state.profile && state.profile.name === state.session.user) ? (state.profile.details || {}) : {};
      const res = await api('/site/settings', {
        username: newName,
        reset_avatar: false,
        details: curDetails,
      });
      state.session = await api('/site/session');
      state.name = res.name;
      renderSession();
      if ($('settings-username-display')) $('settings-username-display').textContent = res.name;
      closeRenameDialog();
      showSettingsStatus(`Username changed to ${res.name}`, 'info');
    } catch (err) {
      if (errEl) {
        errEl.textContent = err.message;
        errEl.hidden = false;
      }
    } finally {
      if (btn) btn.disabled = false;
    }
  });
}

// Avatar management
async function handleAvatarFile(file) {
  if (!file) return;
  if (file.size > 2 * 1024 * 1024 || !['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(file.type)) {
    showSettingsStatus('Choose a PNG, JPEG, GIF or WebP up to 2 MB.', 'error');
    if ($('avatar-file')) $('avatar-file').value = '';
    return;
  }
  const reader = new FileReader();
  reader.onload = async () => {
    state.avatarUpload = reader.result;
    state.resetAvatar = false;
    if ($('settings-avatar')) $('settings-avatar').src = reader.result;
    const dropZone = $('avatar-drop-zone');
    if (dropZone) dropZone.classList.add('js-account-edit-avatar--saving');
    try {
      await saveSettingsSection({}, 'save-status-avatar', 'Avatar updated!');
      state.avatarVersion = Date.now();
      renderSession();
    } catch (_) {}
    finally {
      if (dropZone) dropZone.classList.remove('js-account-edit-avatar--saving');
    }
  };
  reader.onerror = () => {
    showSettingsStatus('The image could not be read.', 'error');
  };
  reader.readAsDataURL(file);
}

on('btn-upload-avatar', 'click', () => $('avatar-file')?.click());
on('avatar-file', 'change', () => {
  handleAvatarFile($('avatar-file')?.files?.[0]);
});

const avatarDropZone = $('avatar-drop-zone');
if (avatarDropZone) {
  avatarDropZone.addEventListener('dragover', e => {
    e.preventDefault();
    avatarDropZone.classList.add('js-account-edit-avatar--hover');
  });
  avatarDropZone.addEventListener('dragleave', e => {
    e.preventDefault();
    avatarDropZone.classList.remove('js-account-edit-avatar--hover');
  });
  avatarDropZone.addEventListener('drop', e => {
    e.preventDefault();
    avatarDropZone.classList.remove('js-account-edit-avatar--hover');
    const file = e.dataTransfer?.files?.[0];
    if (file) handleAvatarFile(file);
  });
}

on('reset-avatar', 'click', async () => {
  state.avatarUpload = null;
  state.resetAvatar = true;
  if ($('avatar-file')) $('avatar-file').value = '';
  if ($('settings-avatar')) $('settings-avatar').src = fallbackAvatar;
  try {
    await saveSettingsSection({}, 'save-status-avatar', 'Avatar reset!');
    state.avatarVersion = Date.now();
    renderSession();
  } catch (_) {}
});

// Signature live preview & BBCode toolbar
function updateSignaturePreview() {
  const ta = $('settings-about');
  const preview = $('settings-signature-preview');
  if (!preview) return;
  const raw = ta ? ta.value.trim() : '';
  if (raw) {
    preview.innerHTML = parseBBCode(raw);
  } else {
    preview.innerHTML = '<span style="color:hsl(var(--hsl-f1));font-style:italic;">Signature preview</span>';
  }
}

function insertBBCode(openTag, closeTag = '', placeholder = '') {
  const ta = $('settings-about');
  if (!ta) return;
  const start = ta.selectionStart;
  const end = ta.selectionEnd;
  const sel = ta.value.substring(start, end) || placeholder;
  const replacement = openTag + sel + closeTag;
  ta.focus();
  ta.setRangeText(replacement, start, end, 'select');
  updateSignaturePreview();
}

function initBBCodeToolbar() {
  const toolbar = $('settings-bbcode-toolbar');
  if (!toolbar) return;
  toolbar.querySelectorAll('button[data-tag]').forEach(btn => {
    btn.addEventListener('click', () => {
      const tag = btn.dataset.tag;
      switch (tag) {
        case 'b': insertBBCode('[b]', '[/b]', 'bold text'); break;
        case 'i': insertBBCode('[i]', '[/i]', 'italic text'); break;
        case 's': insertBBCode('[strike]', '[/strike]', 'strike text'); break;
        case 'heading': insertBBCode('[heading]', '[/heading]', 'Heading'); break;
        case 'url': {
          const url = prompt('Enter the link URL (e.g. https://...):', 'https://');
          if (url) insertBBCode(`[url=${url}]`, '[/url]', 'link text');
          break;
        }
        case 'quote': insertBBCode('[quote]', '[/quote]', 'Quote text'); break;
        case 'list': insertBBCode('[list]\n[*] ', '\n[*] Item 2\n[/list]', 'Item 1'); break;
        case 'list-ol': insertBBCode('[list=1]\n[*] ', '\n[*] Item 2\n[/list]', 'Item 1'); break;
        case 'img': {
          const url = prompt('Enter the image URL (e.g. https://...):', 'https://');
          if (url) insertBBCode(`[img]${url}[/img]`);
          break;
        }
      }
    });
  });

  const sizeSelect = $('bbcode-font-size');
  if (sizeSelect) {
    sizeSelect.addEventListener('change', () => {
      const val = sizeSelect.value;
      if (val) {
        insertBBCode(`[size=${val}]`, '[/size]', 'text');
        sizeSelect.value = '';
      }
    });
  }

  on('settings-about', 'input', updateSignaturePreview);
}
initBBCodeToolbar();

// Section update buttons
on('btn-save-profile', 'click', async () => {
  const btn = $('btn-save-profile');
  if (btn) btn.disabled = true;
  try {
    await saveSettingsSection({
      country: $('settings-country')?.value || '',
      location: $('settings-location')?.value || '',
      interests: $('settings-interests')?.value || '',
      occupation: $('settings-occupation')?.value || '',
      twitter: $('settings-twitter')?.value || '',
      discord: $('settings-discord')?.value || '',
      website: $('settings-website')?.value || '',
    }, 'save-status-profile');
  } catch (_) {}
  finally { if (btn) btn.disabled = false; }
});

on('btn-save-signature', 'click', async () => {
  const btn = $('btn-save-signature');
  if (btn) btn.disabled = true;
  try {
    await saveSettingsSection({ about: $('settings-about')?.value || '' }, 'save-status-signature');
  } catch (_) {}
  finally { if (btn) btn.disabled = false; }
});

on('btn-save-playstyles', 'click', async () => {
  const btn = $('btn-save-playstyles');
  if (btn) btn.disabled = true;
  try {
    const devices = [...document.querySelectorAll('input[name=device]:checked')].map(i => i.value);
    const selectedMode = document.querySelector('input[name="playmode"]:checked')?.value || 'osu';
    state.mode = selectedMode;
    updateModeSelector();
    await saveSettingsSection({ devices }, 'save-status-playstyles');
  } catch (_) {}
  finally { if (btn) btn.disabled = false; }
});

on('btn-save-order', 'click', async () => {
  const btn = $('btn-save-order');
  if (btn) btn.disabled = true;
  try {
    await saveSettingsSection({ section_order: state.sectionOrder }, 'save-status-order');
  } catch (_) {}
  finally { if (btn) btn.disabled = false; }
});

// Dashboard header and navigation links
on('dash-link-dashboard', 'click', e => {
  e.preventDefault();
  showProfilePage(state.session?.user, true);
});
on('dash-link-account', 'click', e => {
  e.preventDefault();
  showSettingsPage('settings-sec-profile', true);
});
on('dash-link-import', 'click', e => {
  e.preventDefault();
  showSettingsPage('settings-sec-scores', true);
});

on('edit-profile', 'click', () => showSettingsPage('settings-sec-profile', true));
on('edit-profile-details', 'click', () => showSettingsPage('settings-sec-profile', true));
on('manage-profile', 'click', () => showSettingsPage('settings-sec-profile', true));
on('edit-about', 'click', () => showSettingsPage('settings-sec-signature', true));
on('account-import-scores', 'click', () => {
  closeAccount();
  showSettingsPage('settings-sec-scores', true);
});

function initAutoSubmitFields() {
  const textFields = [
    { id: 'settings-location', field: 'location' },
    { id: 'settings-interests', field: 'interests' },
    { id: 'settings-occupation', field: 'occupation' },
    { id: 'settings-twitter', field: 'twitter' },
    { id: 'settings-discord', field: 'discord' },
    { id: 'settings-website', field: 'website' },
  ];

  textFields.forEach(({ id, field }) => {
    const input = $(id);
    if (!input) return;
    const entry = input.closest('.account-edit-entry');

    const handleSave = async () => {
      const val = input.value.trim();
      if (input._lastSavedValue === val) return;
      input._lastSavedValue = val;
      if (entry) entry.setAttribute('data-account-edit-state', 'saving');
      try {
        await saveSettingsSection({ [field]: val });
        if (entry) {
          entry.setAttribute('data-account-edit-state', 'saved');
          setTimeout(() => {
            if (entry.getAttribute('data-account-edit-state') === 'saved') {
              entry.removeAttribute('data-account-edit-state');
            }
          }, 2500);
        }
      } catch (err) {
        if (entry) entry.removeAttribute('data-account-edit-state');
      }
    };

    input.addEventListener('blur', handleSave);
    input.addEventListener('keydown', e => {
      if (e.key === 'Enter') {
        e.preventDefault();
        input.blur();
      }
    });
  });

  const countrySel = $('settings-country');
  if (countrySel) {
    const entry = countrySel.closest('.account-edit-entry');
    countrySel.addEventListener('change', async () => {
      const val = countrySel.value;
      updateSettingsCountryFlag();
      if (countrySel._lastSavedValue === val) return;
      countrySel._lastSavedValue = val;
      if (entry) entry.setAttribute('data-account-edit-state', 'saving');
      try {
        await saveSettingsSection({ country: val });
        if (entry) {
          entry.setAttribute('data-account-edit-state', 'saved');
          setTimeout(() => {
            if (entry.getAttribute('data-account-edit-state') === 'saved') {
              entry.removeAttribute('data-account-edit-state');
            }
          }, 2500);
        }
      } catch (err) {
        if (entry) entry.removeAttribute('data-account-edit-state');
      }
    });
  }

  // Playstyle devices checkboxes auto-submit
  document.querySelectorAll('input[name=device]').forEach(cb => {
    cb.addEventListener('change', async () => {
      const entry = $('entry-devices') || cb.closest('.account-edit-entry');
      if (entry) entry.setAttribute('data-account-edit-state', 'saving');
      try {
        const devices = [...document.querySelectorAll('input[name=device]:checked')].map(i => i.value);
        await saveSettingsSection({ devices });
        if (entry) {
          entry.setAttribute('data-account-edit-state', 'saved');
          setTimeout(() => {
            if (entry.getAttribute('data-account-edit-state') === 'saved') {
              entry.removeAttribute('data-account-edit-state');
            }
          }, 2500);
        }
      } catch (err) {
        if (entry) entry.removeAttribute('data-account-edit-state');
      }
    });
  });

  // Default mode radio auto-submit
  document.querySelectorAll('input[name=playmode]').forEach(rb => {
    rb.addEventListener('change', async () => {
      const entry = $('entry-playmode') || rb.closest('.account-edit-entry');
      if (entry) entry.setAttribute('data-account-edit-state', 'saving');
      try {
        state.mode = rb.value;
        updateModeSelector();
        await saveSettingsSection({ playmode: rb.value });
        if (entry) {
          entry.setAttribute('data-account-edit-state', 'saved');
          setTimeout(() => {
            if (entry.getAttribute('data-account-edit-state') === 'saved') {
              entry.removeAttribute('data-account-edit-state');
            }
          }, 2500);
        }
      } catch (err) {
        if (entry) entry.removeAttribute('data-account-edit-state');
      }
    });
  });

  // Privacy toggles auto-submit simulation
  ['pref-pm-friends', 'pref-chat-filter', 'pref-share-city'].forEach(id => {
    const el = $(id);
    if (!el) return;
    el.addEventListener('change', () => {
      const entry = $('entry-privacy') || el.closest('.account-edit-entry');
      if (entry) {
        entry.setAttribute('data-account-edit-state', 'saving');
        setTimeout(() => {
          entry.setAttribute('data-account-edit-state', 'saved');
          setTimeout(() => entry.removeAttribute('data-account-edit-state'), 2500);
        }, 200);
      }
    });
  });

  // API Key copy button
  const copyBtn = $('btn-copy-api-key');
  if (copyBtn) {
    copyBtn.addEventListener('click', () => {
      const keyInput = $('settings-api-key');
      if (keyInput) {
        navigator.clipboard?.writeText(keyInput.value);
        const textEl = $('btn-copy-api-text');
        if (textEl) {
          textEl.textContent = 'copied!';
          setTimeout(() => { textEl.textContent = 'copy'; }, 2000);
        }
      }
    });
  }
}
initAutoSubmitFields();

function formatHighestDate(dateStr){
  if(!dateStr)return'';
  const parts=String(dateStr).split('T')[0].split('-');
  if(parts.length===3){
    const months=['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
    const month=months[Number(parts[1])-1]||parts[1];
    return`${Number(parts[2])} ${month} ${parts[0]}`;
  }
  return dateStr;
}

function getHighestRank(p){
  if(p.rank_highest)return p.rank_highest;
  let best=null;
  if(p.rank_history&&p.rank_history.length){
    for(const [date,r] of p.rank_history){
      if(r>0&&(!best||r<best.rank)){
        best={rank:r,updated_at:date};
      }
    }
  }
  if(p.global_rank&&(!best||p.global_rank<best.rank)){
    best={rank:p.global_rank,updated_at:new Date().toISOString().slice(0,10)};
  }
  return best;
}

function getMonotonePath(pts){
  const n=pts.length;
  if(!n)return'';
  if(n===1)return`M 0,${pts[0].y.toFixed(2)} L 600,${pts[0].y.toFixed(2)}`;
  if(n===2)return`M ${pts[0].x.toFixed(2)},${pts[0].y.toFixed(2)} L ${pts[1].x.toFixed(2)},${pts[1].y.toFixed(2)}`;
  const dx=[],dy=[],s=[];
  for(let i=0;i<n-1;i++){
    const h=pts[i+1].x-pts[i].x,v=pts[i+1].y-pts[i].y;
    dx.push(h);dy.push(v);s.push(h!==0?v/h:0);
  }
  const t=new Array(n).fill(0);
  for(let i=1;i<n-1;i++){
    const s0=s[i-1],s1=s[i],h0=dx[i-1],h1=dx[i];
    if(s0*s1<=0){t[i]=0;}
    else{
      const p=(s0*h1+s1*h0)/(h0+h1);
      const sign=s0<0?-1:1;
      t[i]=2*sign*Math.min(Math.abs(s0),Math.abs(s1),0.5*Math.abs(p));
    }
  }
  t[0]=(3*s[0]-t[1])/2;
  t[n-1]=(3*s[n-2]-t[n-2])/2;
  let d=`M ${pts[0].x.toFixed(2)},${pts[0].y.toFixed(2)}`;
  for(let i=0;i<n-1;i++){
    const x0=pts[i].x,y0=pts[i].y,x1=pts[i+1].x,y1=pts[i+1].y;
    const h=(x1-x0)/3;
    const cp1x=x0+h,cp1y=y0+h*t[i],cp2x=x1-h,cp2y=y1-h*t[i+1];
    d+=` C ${cp1x.toFixed(2)},${cp1y.toFixed(2)} ${cp2x.toFixed(2)},${cp2y.toFixed(2)} ${x1.toFixed(2)},${y1.toFixed(2)}`;
  }
  return d;
}

function renderPerformanceGraph(){
  const data=state.profile.rank_history||[];
  if(!data.length){$('rank-history').textContent='';return;}
  const min=Math.min(...data.map(x=>x[1])),max=Math.max(...data.map(x=>x[1]));
  const pts=data.map((x,i)=>({
    x:data.length===1?300:(i*600)/(data.length-1),
    y:max===min?35:10+((x[1]-min)/(max-min))*52
  }));
  const pathD=getMonotonePath(pts);
  $('rank-history').innerHTML=`<div class="rank-chart-wrap"><svg class="rank-chart-svg" viewBox="0 0 600 75" preserveAspectRatio="none" role="img" aria-label="Recorded osu!daily rank history"><path d="${pathD}" fill="none" stroke="#ffcc22" stroke-width="2" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round"/></svg><div class="rank-chart-hover" hidden><div class="rank-chart-hover__line"></div><div class="rank-chart-hover__circle"></div><div class="rank-chart-hover__tooltip"></div></div></div><span class="performance-caption">osu!daily rank history · ${data.length===1?'tracking started today':esc(data[0][0])+' – '+esc(data[data.length-1][0])}</span>`;

  const wrap=$('rank-history').querySelector('.rank-chart-wrap');
  const hover=wrap.querySelector('.rank-chart-hover');
  const hoverLine=wrap.querySelector('.rank-chart-hover__line');
  const hoverCircle=wrap.querySelector('.rank-chart-hover__circle');
  const tooltip=wrap.querySelector('.rank-chart-hover__tooltip');

  function formatRankTime(dateStr,idx,total){
    if(!dateStr)return'now';
    const parts=String(dateStr).split('-');
    if(parts.length===3){
      const d=new Date(Number(parts[0]),Number(parts[1])-1,Number(parts[2]));
      const now=new Date();
      const today=new Date(now.getFullYear(),now.getMonth(),now.getDate());
      const diffDays=Math.round((today-d)/86400000);
      if(diffDays<=0)return'now';
      if(diffDays===1)return'1 day ago';
      return`${diffDays} days ago`;
    }
    const daysAgo=total-1-idx;
    if(daysAgo<=0)return'now';
    if(daysAgo===1)return'1 day ago';
    return`${daysAgo} days ago`;
  }

  function updateHover(clientX){
    const rect=wrap.getBoundingClientRect();
    if(!rect.width)return;
    const relX=Math.max(0,Math.min(rect.width,clientX-rect.left));
    const ratio=relX/rect.width;
    const idx=Math.max(0,Math.min(data.length-1,Math.round(ratio*(data.length-1))));
    const pt=pts[idx];
    const item=data[idx];
    const pctX=(pt.x/600)*100;
    const pctY=(pt.y/75)*100;

    hover.hidden=false;
    hoverLine.style.left=`${pctX}%`;
    hoverCircle.style.left=`${pctX}%`;
    hoverCircle.style.top=`${pctY}%`;
    tooltip.innerHTML=`<div class="rank-chart-hover__title"><strong>Global Ranking</strong> #${num(item[1])}</div><div class="rank-chart-hover__time">${formatRankTime(item[0],idx,data.length)}</div>`;
    tooltip.style.left=`${pctX}%`;
    tooltip.style.top=`${pctY}%`;
    tooltip.style.marginTop='-12px';
    tooltip.style.transform=pctX<18?'translate(0, -100%)':pctX>82?'translate(-100%, -100%)':'translate(-50%, -100%)';
  }

  wrap.addEventListener('mousemove',e=>updateHover(e.clientX));
  wrap.addEventListener('mouseleave',()=>{hover.hidden=true;});
  wrap.addEventListener('touchmove',e=>{if(e.touches&&e.touches[0])updateHover(e.touches[0].clientX);},{passive:true});
  wrap.addEventListener('touchend',()=>{hover.hidden=true;});
}
function renderSectionOrder(){
  const sectionLabels = { 'me': 'me!', 'top-ranks': 'ranks', 'historical': 'historical', 'beatmaps': 'beatmaps', 'medals': 'medals', 'recent-activity': 'recent' };
  $('section-order-list').innerHTML=state.sectionOrder.map((id,i)=>{
    const link=document.querySelector('.section-tabs a[href="#'+id+'"]');
    const name=link?link.textContent:(sectionLabels[id]||id);
    return `<div class="section-order-row"><span>${esc(name)}</span><button type="button" data-order="${i}" data-direction="-1" aria-label="Move ${esc(id)} up" ${i===0?'disabled':''}>↑</button><button type="button" data-order="${i}" data-direction="1" aria-label="Move ${esc(id)} down" ${i===state.sectionOrder.length-1?'disabled':''}>↓</button></div>`;
  }).join('');
}
$('section-order-list').addEventListener('click',event=>{
  const button=event.target.closest('[data-order]');if(!button)return;
  const i=Number(button.dataset.order),j=i+Number(button.dataset.direction);
  if(j<0||j>=state.sectionOrder.length)return;
  [state.sectionOrder[i],state.sectionOrder[j]]=[state.sectionOrder[j],state.sectionOrder[i]];renderSectionOrder();
});
$('profile-content').addEventListener('click',async event=>{
  const button=event.target.closest('[data-delete-score]');if(!button)return;
  if(!confirm('Delete this score from your profile? Your pp and statistics will be recalculated. A recovery copy is retained on the server.'))return;
  button.disabled=true;
  try{await api('/site/scores/delete',{id:button.dataset.deleteScore});await loadProfile(state.name);}
  catch(error){showMessage(error.message);button.disabled=false;}
});
