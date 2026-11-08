  // ==== part: storage ====

  // ---- Stroke symbols for the parts below (SF Symbols and Material icons, drawn as lines) ----
  var SYM = {
    folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
    pin: 'M9 3h6l-1 6 3 3H7l3-3zM12 12v9',
    unpin: 'M9 3h6l-1 6 3 3H7l3-3zM12 12v9M4 4l16 16',
    cloud: 'M7 18a4 4 0 0 1-.5-8A6 6 0 0 1 18 9a4.5 4.5 0 0 1 0 9z',
    up: 'M12 20V5M6 11l6-6 6 6',
    down: 'M12 4v15M6 13l6 6 6-6',
    pause: 'M8 5v14M16 5v14',
    play: 'M7 4l13 8-13 8z',
    sidebar: 'M3 5h18v14H3zM15 5v14',
    clock: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2',
    xcircle: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM9 9l6 6M15 9l-6 6',
    copy: 'M8 8h12v12H8zM4 16V4h12',
    left: 'M15 5l-7 7 7 7',
    right: 'M9 5l7 7-7 7',
    refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5',
    photo: 'M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M15 9h.01',
    globe: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18',
    sync: 'M4 12a8 8 0 0 1 14-5.3M20 4v4h-4M20 12a8 8 0 0 1-14 5.3M4 20v-4h4',
    moveToCloud: 'M12 4v12M7 11l5 5 5-5M5 20h14',
    makeLocal: 'M14 4h6v6M20 4l-9 9M18 14v6H4V6h6',
    export: 'M12 15V3M7 8l5-5 5 5M5 12v8h14v-8',
    saveAs: 'M12 3v12M7 10l5 5 5-5M5 12v8h14v-8',
    grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
    expand: 'M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7',
    close: 'M6 6l12 12M18 6L6 18',
    back: 'M20 12H5M11 6l-6 6 6 6',
    more: 'M12 5h.01M12 12h.01M12 19h.01',
    check: 'M5 12l5 5 9-10',
    trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13',
    edit: 'M4 20h4L19 9l-4-4L4 16zM14 6l4 4'
  };
  function Sym(props) {
    var s = Number(props.size || 14);
    return h('svg', { width: s, height: s, viewBox: '0 0 24 24', 'aria-hidden': true, className: cx('bae-sym', props.className), fill: 'none', stroke: 'currentColor', strokeWidth: props.weight || 2, strokeLinecap: 'round', strokeLinejoin: 'round', style: props.style }, h('path', { d: SYM[props.name] || '' }));
  }
  function list(s) { return String(s || '').split(',').map(function (x) { return x.trim(); }).filter(Boolean); }

  // ---- Storage Manager (macOS) ----
  // The window: title, Filter field, All / Local / In storage, the Inspector toggle, then the release list and the inspector.
  function MacStorageWindow(props) {
    var k = kids(props.children);
    return h('div', { className: 'bae-m-window bae-m-storewin', style: { width: Number(props.width || 1100), height: Number(props.height || 660) } },
      h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
      h('header', { className: 'bae-m-storewin-bar' },
        h('span', { className: 'bae-m-storewin-title' }, 'Storage Manager'),
        h('span', { style: { flex: 1 } }),
        h(MacSegmentedControl, { options: 'All, Local, In storage', selected: props.filter || 'All' }),
        h('span', { style: { flex: 1 } }),
        h('span', { className: 'bae-m-storewin-filter' }, h(Glyph, { set: 'mac', name: 'search', size: 12 }), h('span', { className: props.search ? null : 'bae-m-search-prompt' }, props.search || 'Filter')),
        h('span', { className: cx('bae-m-iconbtn', props.inspector && 'bae-m-iconbtn-on'), title: 'Inspector' }, h(Sym, { name: 'sidebar', size: 15 }))),
      h('div', { className: 'bae-m-storewin-body' },
        h('div', { className: 'bae-m-storewin-list' }, k[0]),
        props.inspector ? h('div', { className: 'bae-m-storewin-inspector' }, k.slice(1)) : null));
  }
  // An AppKit table: columns "Title:width[:right][:up|:down]", rows as MacTableRow.
  function cols(s) { return list(s).map(function (c) { var p = c.split(':'); return { title: p[0], w: Number(p[1] || 100), right: p.indexOf('right') > 1, sort: p.indexOf('up') > 1 ? 'up' : p.indexOf('down') > 1 ? 'down' : null }; }); }
  function MacTable(props) {
    var cs = cols(props.columns);
    var tmpl = cs.map(function (c, i) { return i === 0 ? 'minmax(' + c.w + 'px, 1fr)' : c.w + 'px'; }).join(' ');
    return h('div', { className: 'bae-m-table', style: { '--bae-cols': tmpl } },
      h('div', { className: 'bae-m-table-head' }, cs.map(function (c, i) { return h('span', { key: i, className: cx('bae-m-table-th', c.right && 'bae-m-table-right') }, c.title, c.sort ? h(Glyph, { set: 'mac', name: 'down', size: 8, className: 'bae-muted' }) : null); })),
      h('div', { className: 'bae-m-table-body' }, kids(props.children).map(function (r, i) { return h('div', { key: i, className: cx('bae-m-table-tr', i % 2 && 'bae-m-table-alt') }, r); })),
      props.empty ? h('div', { className: 'bae-m-table-empty' }, h('div', { className: 'bae-m-cu-title' }, props.empty)) : null);
  }
  function MacTableRow(props) {
    if (props.placeholder) return h('div', { className: 'bae-m-table-row' }, h('span', { className: 'bae-m-table-td' }, h('span', { className: 'bae-ph', style: { width: 180, height: 14, borderRadius: 3 } })));
    return h('div', { className: cx('bae-m-table-row', props.selected && 'bae-m-table-selected') }, kids(props.children).map(function (c, i) { return h('span', { key: i, className: 'bae-m-table-td' }, c); }));
  }
  function MacReleaseCell(props) { return h('span', { className: 'bae-m-relcell' }, h(Artwork, { title: props.title, size: 24, radius: 4 }), h('span', { className: 'bae-ellipsis' }, props.title)); }
  // The Storage column, as StorageStateLabel draws it: one chip, or several in a row. At rest: Local, Pinned, In storage. Failed: Download failed, Move failed (`file`), Couldn't make local. Waiting: Queued or Queued to make local, `ahead`, then the `wait`. Moving: Uploading, Pinning and Making local… fill with `progress` and take a `wait` after them; Copying here.
  function MacStorageChip(props) {
    var s = props.state || 'cloud';
    var chip = function (tone, label) { return h(StatusChip, { tone: tone }, label); };
    var wait = props.wait ? chip('warning', props.wait) : null;
    var row = function (parts) { return h('span', { className: 'bae-m-chips' }, parts.filter(Boolean)); };
    var queued = function (label) { return row([h(React.Fragment, { key: 'q' }, chip('activity', label)), Number(props.ahead || 0) > 0 ? h(React.Fragment, { key: 'a' }, chip('neutral', props.ahead + ' ahead')) : null, wait ? h(React.Fragment, { key: 'w' }, wait) : null]); };
    if (s === 'uploading' || s === 'pinning' || s === 'detaching') {
      var label = s === 'uploading' ? 'Uploading' : s === 'pinning' ? 'Pinning' : 'Making local…';
      var filling = h('span', { className: cx('bae-m-progchip', s === 'uploading' ? 'bae-m-progchip-up' : 'bae-m-progchip-pin') }, h('span', { className: 'bae-m-progchip-fill', style: { width: (Number(props.progress || 0.4) * 100) + '%' } }), h('span', { className: 'bae-m-progchip-label' }, label));
      return wait ? row([h(React.Fragment, { key: 'p' }, filling), h(React.Fragment, { key: 'w' }, wait)]) : filling;
    }
    if (s === 'transfer') return chip('activity', props.label || 'Pinning for offline');
    if (s === 'downloadFailed') return chip('danger', 'Download failed');
    if (s === 'moveFailed') return chip('danger', 'Move failed: ' + (props.file || '01 Harbor Lights.flac') + ' changed since import');
    if (s === 'makeLocalFailed') return chip('danger', 'Couldn’t make local');
    if (s === 'queued') return queued('Queued');
    if (s === 'makeLocalQueued') return queued('Queued to make local');
    if (s === 'copying') return chip('activity', 'Copying here');
    var icon = { local: 'folder', pinned: 'pin', cloud: 'cloud' }[s];
    return h(StatusChip, { tone: 'neutral' }, h(Sym, { name: icon, size: 10 }), { local: 'Local', pinned: 'Pinned', cloud: 'In storage' }[s]);
  }
  // DownloadRow's failed row in the inspector: the kind's icon, title and detail, how long ago it was queued, the cancel mark; under them the failure chip and Retry.
  function MacDownloadRow(props) {
    var makeLocal = props.kind === 'makeLocal';
    return h('div', { className: 'bae-m-dlrow' },
      h('div', { className: 'bae-m-dlrow-top' },
        h(Sym, { name: makeLocal ? 'folder' : 'down', size: 16, className: 'bae-muted' }),
        h('div', { className: 'bae-m-dlrow-text' }, h('span', { className: 'bae-ellipsis' }, props.title || 'Glass Harbor'), h('span', { className: 'bae-muted bae-num bae-m-dlrow-detail' }, props.detail || '9 files · 356 MB')),
        h('span', { className: 'bae-muted bae-m-dlrow-ago' }, props.ago || '2m ago'),
        h('span', { className: 'bae-muted', title: 'Cancel this download' }, h(Sym, { name: 'xcircle', size: 15 }))),
      h('div', { className: 'bae-m-dlrow-badge' },
        h(StatusChip, { tone: 'danger' }, h(StatusGlyph, { tone: 'danger', set: 'mac', size: 11 }), makeLocal ? 'Couldn’t make local' : 'Failed'),
        h(MacButton, { size: 'small' }, 'Retry')));
  }
  // Under the list: how many releases, the total, then one rate and Pause per queue with work.
  function MacStorageFooter(props) {
    var q = function (kind, rate, label) { return h('span', { className: 'bae-m-storefoot-q' }, h(Sym, { name: kind === 'exports' ? 'export' : kind === 'uploads' ? 'up' : 'down', size: 11, className: kind === 'uploads' ? 'bae-activity' : kind === 'pins' ? 'bae-pinning' : null }), rate ? h('span', { className: 'bae-num' }, rate) : null, h('span', { title: label }, h(Sym, { name: 'pause', size: 10 }))); };
    return h('div', { className: 'bae-m-storefoot' },
      h('span', null, props.releases || '8 releases', props.total ? ' · Total: ' + props.total : ''),
      h('span', { style: { flex: 1 } }),
      props.uploads ? q('uploads', props.uploads, 'Pause uploads') : null,
      props.pins ? q('pins', props.pins, 'Pause pinning') : null,
      props.exports ? q('exports', props.exports, 'Pause exports') : null);
  }
  // A release's upload or pin in the inspector: the counts line with Cancel, then bytes, rate and a bar.
  function MacTransferSummary(props) {
    var kind = props.kind || 'upload';
    var parts = list(props.counts);
    return h('div', { className: 'bae-m-tsum' },
      h('div', { className: 'bae-m-tsum-line' },
        h('span', { className: 'bae-m-tsum-counts' }, parts.map(function (p, i) { return h(React.Fragment, { key: i }, i ? h('span', { className: 'bae-muted' }, ' · ') : null, h('span', { className: i === 0 ? cx('bae-strong', kind === 'upload' ? 'bae-activity' : 'bae-pinning') : i === parts.length - 1 ? 'bae-muted' : null }, p)); })),
        h('span', { style: { flex: 1 } }), props.cancel === false ? null : h(MacButton, { size: 'small' }, 'Cancel')),
      props.bytes ? h(MacTransferReadout, { kind: kind, bytes: props.bytes, rate: props.rate, progress: props.progress }) : null);
  }
  function MacTransferReadout(props) {
    return h('div', { className: 'bae-m-readout' },
      h('div', { className: 'bae-m-readout-line bae-num' }, h('span', null, props.bytes), h('span', null, props.rate)),
      h('div', { className: 'bae-m-track' }, h('div', { className: props.kind === 'pin' ? 'bae-pinning-bg' : 'bae-activity-bg', style: { width: (Number(props.progress || 0.3) * 100) + '%' } })));
  }
  // One file in the inspector: a mark for queued (dashed), sending or pinning (a ring filling), its name and size.
  function MacFileRow(props) {
    var s = props.state || 'rest';
    var p = Number(props.progress || 0.4), C = 2 * Math.PI * 6;
    var mark = s === 'rest' ? h('span', { style: { width: 16 } }) :
      s === 'queued' ? h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', className: 'bae-muted' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'currentColor', strokeWidth: 1.75, strokeDasharray: '2 2' })) :
      h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', className: s === 'pinning' ? 'bae-pinning' : 'bae-activity' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--hairline)', strokeWidth: 2 }), h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeDasharray: (C * p) + ' ' + C, transform: 'rotate(-90 8 8)' }));
    return h('div', { className: cx('bae-m-filerow', props.selected && 'bae-m-filerow-selected') }, mark,
      h('span', { className: cx('bae-ellipsis', s === 'queued' && 'bae-muted', s === 'sending' && 'bae-activity', s === 'pinning' && 'bae-pinning'), style: { flex: 1 } }, props.name),
      h('span', { className: 'bae-muted bae-num' }, props.size));
  }
  function MacFileDetails(props) {
    var row = function (l, v) { return v ? [h('span', { key: l + 'l', className: 'bae-muted' }, l), h('span', { key: l + 'v', className: l === 'Size' ? 'bae-num' : null }, v)] : null; };
    return h('div', { className: 'bae-m-filedet' },
      h('div', { className: 'bae-m-filedet-head' }, h('span', { className: 'bae-ellipsis' }, props.name), h('span', { className: 'bae-muted', title: 'Close file details' }, h(Sym, { name: 'xcircle', size: 15 }))),
      h('div', { className: 'bae-m-filedet-grid' }, row('Folder', props.folder), row('Kind', props.kind), row('Format', props.format), row('Size', props.size)));
  }
  // BaeKit's ErrorDetailDisclosure: the message in its tone, the fault with Copy details, then Details.
  // Drawn in a notice of its tone, the notice's glyph leading: the line, then Details, which opens on an inset panel of the causes, one a line (chain, ending in the fault), Copy details in its corner.
  // bare drops the notice, for a disclosure inside a notice that already holds it.
  function MacErrorDisclosure(props) {
    var tone = props.tone || 'danger';
    var body = h('div', { className: 'bae-m-errdis' },
      h('div', { className: cx('bae-m-errdis-msg', 'bae-tone-' + tone) }, h('span', null, props.message)),
      props.fault ? h('div', { className: 'bae-m-errdis-more' }, h(Chevron, { className: props.open ? 'bae-m-errdis-open' : null }), 'Details') : null,
      props.fault && props.open ? h('div', { className: 'bae-m-errdis-panel' }, h('div', { className: 'bae-m-errdis-chain' }, props.chain || props.fault), h('span', { className: 'bae-m-errdis-copy', title: 'Copy details' }, h(Sym, { name: 'copy', size: 11 }))) : null);
    return props.bare ? body : h(Notice, { tone: tone, set: 'mac' }, body);
  }
  // A popover hanging from a title-bar button, its arrow pointing up at it.
  function MacPopover(props) {
    return h('div', { className: 'bae-m-popover', style: { width: Number(props.width || 360), top: props.top != null ? Number(props.top) : null, right: props.right != null ? Number(props.right) : null } }, h('span', { className: 'bae-m-popover-arrow' }), props.children);
  }
  // BaeKit's SyncActivityList: Needs Attention, Transfers, or up to date; then the store's error and Retry Now.
  function MacSyncActivity(props) {
    var sec = function (title, body) { return h(MacSection, { title: title }, h(MacBox, null, body)); };
    var label = function (icon, text, sub) { return h('div', { className: 'bae-m-actrow' }, h(Sym, { name: icon, size: 15, className: 'bae-accent' }), h('div', null, h('div', null, text), sub ? h('div', { className: 'bae-m-actrow-sub bae-num' }, sub) : null)); };
    var out = [];
    if (props.attention) out.push(h(React.Fragment, { key: 'a' }, sec('Needs Attention', [
      h('div', { key: 'f', className: 'bae-m-actpad' }, h(MacErrorDisclosure, { tone: 'warning', message: props.attention, fault: props.fault })),
      props.record ? h('div', { key: 'r', className: 'bae-m-actpad' }, h('div', { className: 'bae-strong' }, props.record), h(MacErrorDisclosure, { tone: 'neutral', message: props.reason })) : null,
      h('div', { key: 'b', className: 'bae-m-actpad' }, h(MacButton, null, 'Open Sync Settings'))].filter(Boolean))));
    if (props.syncing || props.uploads || props.downloads) out.push(h(React.Fragment, { key: 't' }, sec('Transfers', [
      props.syncing ? h('div', { key: 's', className: 'bae-m-actpad' }, label('sync', 'Syncing library changes…')) : null,
      props.uploads ? h('div', { key: 'u', className: 'bae-m-actpad' }, label('up', props.uploads, props.uploadBytes)) : null,
      props.downloads ? h('div', { key: 'd', className: 'bae-m-actpad' }, label('down', props.downloads)) : null].filter(Boolean))));
    if (!out.length) out.push(h(React.Fragment, { key: 'i' }, sec(null, h('div', { className: 'bae-m-actpad bae-muted' }, 'Everything is up to date.'))));
    if (props.error) out.push(h(React.Fragment, { key: 'e' }, sec(null, [h('div', { key: 'e', className: 'bae-m-actpad' }, h(MacErrorDisclosure, { message: props.error, fault: props.errorFault })), props.retry ? h('div', { key: 'r', className: 'bae-m-actpad' }, h(MacButton, null, 'Retry Now')) : null].filter(Boolean))));
    return h('div', { className: 'bae-m-actlist' }, out);
  }

  // ---- Android: sync activity and Downloads ----
  function AndroidErrorDisclosure(props) {
    var tone = props.tone || 'danger';
    return h('div', { className: 'bae-a-errdis' },
      h('div', { className: cx('bae-a-errdis-msg', 'bae-tone-' + tone) }, props.message),
      props.fault ? h('div', { className: 'bae-a-errdis-fault' }, h('span', { className: 'bae-mono bae-muted' }, props.fault), h(AndroidIconButton, { icon: 'copy', label: 'Copy details' })) : null);
  }
  // SyncActivity.kt's sheet: the title, Needs Attention, the transfers (the downloads line opens Downloads), or up to date.
  function AndroidSyncActivity(props) {
    var t = function (s, c) { return h('div', { className: c || 'bae-a-act-line' }, s); };
    return h('div', { className: 'bae-a-act' },
      h('div', { className: 'bae-a-act-title' }, 'Sync Activity'),
      props.attention ? h('div', { className: 'bae-a-act-block' }, h('div', { className: 'bae-strong', style: { paddingTop: 4 } }, 'Needs Attention'), h(AndroidErrorDisclosure, { tone: 'warning', message: props.attention, fault: props.fault }),
        props.record ? h('div', null, h('div', { className: 'bae-strong' }, props.record), h('div', { className: 'bae-muted' }, props.reason)) : null,
        h(AndroidButton, { kind: 'text', alignTo: 'text' }, 'Open Sync Settings'), h('div', { className: 'bae-divider' })) : null,
      props.syncing ? t('Syncing library changes…') : null,
      props.uploads ? h('div', null, t(props.uploads), props.uploadBytes ? t(props.uploadBytes, 'bae-a-act-sub') : null) : null,
      props.downloads ? h('div', { className: 'bae-a-act-tap' }, t(props.downloads)) : null,
      !(props.attention || props.syncing || props.uploads || props.downloads) ? t('Everything is up to date.', 'bae-a-act-sub') : null,
      props.error ? h(AndroidErrorDisclosure, { message: props.error, fault: props.errorFault }) : null,
      props.retry ? h(AndroidButton, { kind: 'outlined' }, 'Retry Now') : null);
  }
  function AndroidSegmentedButtons(props) {
    var o = list(props.options);
    return h('div', { className: 'bae-a-seg', role: 'radiogroup' }, o.map(function (x) { var on = x === String(props.selected); return h('span', { key: x, role: 'radio', 'aria-checked': on, className: cx('bae-a-seg-btn', on && 'bae-a-seg-on') }, on ? h(Glyph, { name: 'check', size: 18 }) : null, x); }));
  }
  // The Downloads screen's bar: Back, the title, Retry and Pause or Resume.
  function AndroidDownloadsBar(props) {
    return h('header', { className: 'bae-a-dlbar' }, h(AndroidIconButton, { icon: 'back', label: 'Back' }), h('span', { className: 'bae-a-dlbar-title' }, 'Downloads'),
      h(AndroidButton, { kind: 'text', off: !props.failed }, 'Retry'), h(AndroidButton, { kind: 'text', off: !!props.idle }, props.paused ? 'Resume' : 'Pause'));
  }
  function AndroidDownloadRow(props) {
    var s = props.state || 'queued';
    return h('div', { className: 'bae-a-dlrow2' },
      h('div', { className: 'bae-a-dlrow2-text' },
        h('div', { className: 'bae-a-browse-name' }, props.title || 'Release no longer in library'),
        h('div', { className: 'bae-a-browse-count' }, props.detail),
        s === 'queued' ? h('div', { className: 'bae-a-browse-count' }, 'Waiting to download') :
        s === 'active' ? h('div', null, h('div', { className: 'bae-a-dlbar-track' }, h('div', { style: { width: (Number(props.progress || 0.4) * 100) + '%' } })), h('div', { className: 'bae-a-fine bae-muted' }, props.bytes || '120 MB of 300 MB')) :
        h('div', { className: 'bae-a-error' }, props.error || 'Couldn’t reach your storage.')),
      h(AndroidIcon, { icon: 'close', label: 'Cancel', className: 'bae-a-icon-muted' }));
  }
