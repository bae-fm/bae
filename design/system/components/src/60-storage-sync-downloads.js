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
    external: 'M7 17L17 7M9 7h8v8',
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
  // The window: title, Filter field, All / Local / Cloud, the Inspector toggle, then the release list and the inspector.
  function MacStorageWindow(props) {
    var k = kids(props.children);
    return h('div', { className: 'bae-m-window bae-m-storewin', style: { width: Number(props.width || 1100), height: Number(props.height || 660) } },
      h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
      h('header', { className: 'bae-m-storewin-bar' },
        h('span', { className: 'bae-m-storewin-title' }, 'Storage Manager'),
        h('span', { style: { flex: 1 } }),
        h(MacSegmentedControl, { options: 'All, Local, Cloud', selected: props.filter || 'All' }),
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
  // The Storage column, as StorageStateLabel draws it: one chip. At rest: Local, Pinned, In cloud. Moving: Uploading, Pinning and Moving to This Mac… fill with `progress`. Waiting: Queued to pin, Queued to upload, Queued to move to This Mac; an upload held by a missing original, Waiting for that `file`, in warning (`uploadBlocked`). Failed, in danger: Couldn’t pin, Move failed (`file`), Could not move to This Mac. Nothing says how many wait ahead or what holds the queue back.
  function MacStorageChip(props) {
    var s = props.state || 'cloud';
    var chip = function (tone, label) { return h(StatusChip, { tone: tone }, label); };
    if (s === 'uploading' || s === 'pinning' || s === 'detaching') {
      var label = s === 'uploading' ? 'Uploading' : s === 'pinning' ? 'Pinning' : 'Moving to This Mac…';
      return h('span', { className: cx('bae-m-progchip', s === 'uploading' ? 'bae-m-progchip-up' : 'bae-m-progchip-pin') }, h('span', { className: 'bae-m-progchip-fill', style: { width: (Number(props.progress || 0.4) * 100) + '%' } }), h('span', { className: 'bae-m-progchip-label' }, label));
    }
    if (s === 'downloadFailed') return chip('danger', 'Could not pin');
    if (s === 'moveFailed') return chip('danger', 'Move failed: ' + (props.file || '01 Harbor Lights.flac') + ' changed since import');
    if (s === 'makeLocalFailed') return chip('danger', 'Could not move to This Mac');
    if (s === 'queued') return chip('activity', 'Queued to pin');
    if (s === 'uploadQueued') return chip('activity', 'Queued to upload');
    if (s === 'uploadBlocked') return chip('warning', 'Waiting for ' + (props.file || '01 Harbor Lights.flac'));
    if (s === 'makeLocalQueued') return chip('activity', 'Queued to move to This Mac');
    var icon = { local: 'folder', pinned: 'pin', cloud: 'cloud' }[s];
    return h(StatusChip, { tone: 'neutral' }, h(Sym, { name: icon, size: 10 }), { local: 'Local', pinned: 'Pinned', cloud: 'In cloud' }[s]);
  }

  // DownloadRow's failed row in the inspector: the kind's icon, title and detail, the cancel mark; under them the failure (Couldn’t pin, or Could not move to This Mac) with Details and Retry.
  function MacDownloadRow(props) {
    var makeLocal = props.kind === 'makeLocal';
    return h('div', { className: 'bae-m-dlrow' },
      h(MacItem, { lead: h(Sym, { name: makeLocal ? 'folder' : 'pin', size: 16, className: 'bae-muted' }), trail: h('span', { className: 'bae-muted', title: 'Cancel' }, h(Sym, { name: 'xcircle', size: 15 })) },
        h('div', { className: 'bae-m-dlrow-text' }, h('span', { className: 'bae-ellipsis' }, props.title || 'Glass Harbor'), h('span', { className: 'bae-muted bae-num bae-m-dlrow-detail' }, props.detail || '9 files · 356 MB'))),
      h(MacNotice, { quiet: true, tone: 'danger', title: makeLocal ? 'Could not move to This Mac' : 'Could not pin', secondary: 'Details', action: 'Retry' }));
  }

  // Under the list: how many releases, the total, then per queue with work its rate, or in its place what holds the whole queue back (`uploadsWait`, `pinsWait`: Paused, Waiting for network, Waiting for Wi-Fi, Retrying at 6:40 PM), and Pause, or Resume while the person has paused it (`Paused`).
  function MacStorageFooter(props) {
    var q = function (kind, rate, label) { var wait = props[kind + 'Wait'], paused = wait === 'Paused'; return h('span', { className: 'bae-m-storefoot-q' }, h(Sym, { name: kind === 'exports' ? 'export' : kind === 'uploads' ? 'up' : 'down', size: 11, className: kind === 'uploads' ? 'bae-activity' : kind === 'pins' ? 'bae-pinning' : null }), wait ? h('span', null, wait) : rate ? h('span', { className: 'bae-num' }, rate) : null, h('span', { title: paused ? label.replace('Pause', 'Resume') : label }, h(Sym, { name: paused ? 'play' : 'pause', size: 10 }))); };
    return h('div', { className: 'bae-m-storefoot' },
      h('span', null, props.releases || '8 releases', props.total ? ' · Total: ' + props.total : ''),
      h('span', { style: { flex: 1 } }),
      props.uploads ? q('uploads', props.uploads, 'Pause uploads') : null,
      props.pins ? q('pins', props.pins, 'Pause downloads') : null,
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
      h('div', { className: 'bae-m-bar' }, h('div', { className: props.kind === 'pin' ? 'bae-pinning-bg' : 'bae-activity-bg', style: { width: (Number(props.progress || 0.3) * 100) + '%' } })));
  }
  // One file in the inspector: a mark for queued (dashed), sending or pinning (a ring filling), its name and size.
  function MacFileRow(props) {
    var s = props.state || 'rest';
    var p = Number(props.progress || 0.4), C = 2 * Math.PI * 6;
    var mark = s === 'rest' ? null :
      s === 'queued' ? h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', className: 'bae-muted' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'currentColor', strokeWidth: 1.75, strokeDasharray: '2 2' })) :
      h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', className: s === 'pinning' ? 'bae-pinning' : 'bae-activity' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--hairline)', strokeWidth: 2 }), h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeDasharray: (C * p) + ' ' + C, transform: 'rotate(-90 8 8)' }));
    return h('div', { className: cx('bae-m-filerow', props.selected && 'bae-m-filerow-selected') },
      h(MacItem, { lead: mark, trail: h('span', { className: 'bae-muted bae-num' }, props.size) },
        h('div', { className: cx('bae-ellipsis', s === 'queued' && 'bae-muted', s === 'sending' && 'bae-activity', s === 'pinning' && 'bae-pinning') }, props.name)));
  }
  function MacFileDetails(props) {
    var row = function (l, v) { return v ? [h('span', { key: l + 'l', className: 'bae-muted' }, l), h('span', { key: l + 'v', className: l === 'Size' ? 'bae-num' : null }, v)] : null; };
    return h('div', { className: 'bae-m-filedet' },
      h('div', { className: 'bae-m-filedet-head' }, h('span', { className: 'bae-ellipsis' }, props.name), h('span', { className: 'bae-muted', title: 'Close file details' }, h(Sym, { name: 'xcircle', size: 15 }))),
      h('div', { className: 'bae-m-filedet-grid' }, row('Folder', props.folder), row('Kind', props.kind), row('Format', props.format), row('Size', props.size)));
  }
  // An error's causes, one a line, in an inset panel with Copy in its corner, in an alert (`MacAlert`'s `details`).
  function MacErrorDetails(props) {
    return h('div', { className: 'bae-errdis-panel bae-errdis-panel-hover' }, h('div', { className: 'bae-errdis-chain' }, props.details), h('span', { className: 'bae-errdis-copy', title: 'Copy details' }, h(Sym, { name: 'copy', size: 11 })));
  }
  // macOS's notice: a glyph (`icon: false` drops it), a title and a subtitle saying what to do, then up to two buttons: `action`, the primary one (Retry), and `secondary` (Dismiss, or Details, which opens MacAlert with the error's causes in `details`: a notice never shows the error itself). The buttons sit on the title's line at the right, secondary first; `buttons: 'below'` puts them on a line under the text. `dismiss` adds an × at the end of that line. `layout: 'centered'` is a failed view.
  function MacNotice(props) {
    var tone = props.tone || 'neutral';
    var centered = props.layout === 'centered';
    // Centred, the notice is quiet words: its button's label is secondary text, so nothing in it is brighter than the message; Details is in the accent, as in every notice, apart from the grey title.
    var primary = props.action ? h(MacButton, { key: 'p', muted: centered, tinted: !props.quiet }, props.action) : null;
    var secondary = props.secondary ? h(MacButton, { key: 's', kind: 'borderless' }, props.secondary) : null;
    var dismiss = props.dismiss ? h('span', { key: 'x', className: 'bae-m-notice-dismiss' }, h(MacIconButton, { icon: 'close', size: 14, faint: true, label: 'Dismiss' })) : null;
    // `leave`: the label of the button that leaves a screen the notice fills (Cancel), first in the row, where it is the screen's only way back.
    var leave = centered && props.leave ? h(MacButton, { key: 'l', muted: true }, props.leave) : null;
    if (centered) {
      // The glyph sits beside the title only; a subtitle is its own smaller, grey line under them, so the two lines read as a heading and what to do.
      var title = h(React.Fragment, null,
        h('div', { className: 'bae-notice-centered-title' }, noticeGlyph(tone, props.icon, { centered: true }), h('div', { className: cx('bae-notice-title', props.subtitle && 'bae-notice-title-strong') }, props.title), props.children),
        props.subtitle ? h('div', { className: 'bae-notice-centered-sub' }, props.subtitle) : null);
      return NoticeBox(tone, props.icon, h('div', { className: 'bae-m-notice' }, title, primary || secondary || leave ? h('div', { className: 'bae-notice-centered-row' }, leave, secondary, primary) : null), { platform: 'bae-m-notice-box', quiet: props.quiet, flush: props.flush, centered: true, stage: props.stage });
    }
    // The buttons sit on the text's line at the right, secondary first; `buttons: 'below'` puts them on a line of their own under the text.
    var below = props.buttons === 'below';
    var words = h('div', { key: 'w', className: 'bae-m-notice-words' }, noticeWords(props));
    var buttons = primary || secondary ? h('div', { key: 'b', className: below ? 'bae-m-notice-below' : 'bae-m-notice-buttons' }, secondary, primary) : null;
    // One top row, aligned to the title's line: the words, inline buttons, the ×; `below` buttons are a second row.
    var top = h('div', { className: 'bae-m-notice-top' }, words, below ? null : buttons, dismiss);
    var body = h('div', { className: 'bae-m-notice' }, top, below ? buttons : null);
    // The glyph sits in the row piece's leading column, so a notice among rows shares their text edge.
    var glyph = noticeGlyph(tone, props.icon, { quiet: props.quiet });
    return NoticeBox(tone, false, glyph ? h(MacItem, { lead: glyph }, body) : body, { platform: 'bae-m-notice-box', quiet: props.quiet, flush: props.flush, centered: false, stage: props.stage });
  }

  // A popover hanging from a title-bar button, its arrow pointing up at it.
  function MacPopover(props) {
    return h('div', { className: 'bae-m-popover', style: { width: Number(props.width || 360), top: props.top != null ? Number(props.top) : null, right: props.right != null ? Number(props.right) : null } }, h('span', { className: 'bae-m-popover-arrow' }), h('div', { className: 'bae-m-popover-body' }, props.children));
  }
  // The toolbar's Activity panel (BaeKit's SyncActivityList, grown to hold every problem): Needs Attention lists what bae needs the person for, as notices, each with an × that dismisses it from the panel (the problem stays where it's fixed): the caller's own (children), bae couldn't read sync's status (`error`), and sync's own (signed out, a record it can't send) with Open Sync Settings. Nothing in the panel retries in place. Then Transfers, each a line of words, how far it has got and a bar: Syncing changes (no count, its bar moves), Uploading (`uploads`), Pinning (`pins`), Moving to This Mac (`makingLocal`), each counted in releases done of the batch ("1 of 3 releases"), its bar filled by the bytes done; what holds the uploads or the download queue (`uploadsWait`, `downloadsWait`: Paused, Waiting for network; for uploads also Waiting for Wi-Fi, Retrying at a time) stands in place of the count. Or up to date.
  function MacActivity(props) {
    var sec = function (title, body) { return h(MacSection, { title: title }, h(MacBox, null, body)); };
    // Every row of the panel's boxes has the same inset; a row in the problems' box without a glyph starts on the problems' text edge (MacItem's empty leading column). Transfers is its own box, so its lines start at the box's inset.
    var problemsListed = !!(kids(props.children).length || props.error || props.attention);
    var edge = function (body) { return problemsListed ? h(MacItem, null, body) : body; };
    var row = function (key, body) { return h('div', { key: key, className: 'bae-m-actrow' }, body); };
    var line = function (key, text, sub) { return row(key, h('div', { className: 'bae-m-actline' }, edge(h('div', null, h('div', null, text), sub ? h('div', { className: 'bae-m-actrow-sub' }, sub) : null)))); };
    // A transfer: what it does, then how far (done of total, or what holds the queue), then a bar; syncing has no count, so its bar moves.
    // `held` is the queue's pause state: a queue's line ends in Pause, or Resume while paused; pinning and moving to This Mac are one queue, so both lines' buttons act on it together.
    var xfer = function (key, label, count, progress, held) {
      return row(key, h('div', { className: 'bae-m-actline bae-m-actline-xfer' },
        h('div', { className: 'bae-m-actxfer' }, h('span', null, label), h('span', { className: 'bae-m-actxfer-end' }, count ? h('span', { className: 'bae-muted bae-num' }, count) : null,
          held != null ? h(MacIconButton, { icon: held ? 'play' : 'pause', size: 13, small: true, boxed: true, label: held ? 'Resume' : 'Pause' }) : null)),
        h(ProgressBar, { value: progress == null ? null : Number(progress), className: 'bae-m-actxfer-bar', label: label })));
    };
    var out = [];
    var problems = kids(props.children).map(function (p, i) { return row('c' + i, p); }).concat([
      props.error ? row('e', h(MacNotice, { quiet: true, dismiss: true, tone: 'danger', title: props.error })) : null,
      props.attention ? row('f', h(MacNotice, { quiet: true, dismiss: true, tone: 'warning', title: props.attention, action: 'Open Sync Settings', buttons: 'below' })) : null,
      props.attention && props.record ? line('r', h('span', { className: 'bae-strong' }, props.record), props.reason) : null].filter(Boolean));
    if (problems.length) out.push(h(React.Fragment, { key: 'a' }, sec('Needs Attention', problems)));
    if (props.syncing || props.uploads || props.pins || props.makingLocal) out.push(h(React.Fragment, { key: 't' }, sec('Transfers', [
      props.syncing ? xfer('s', 'Syncing changes', null, null) : null,
      props.uploads ? xfer('u', 'Uploading', props.uploadsWait || props.uploads, props.uploadProgress != null ? props.uploadProgress : 0.4, props.uploadsWait === 'Paused') : null,
      props.pins ? xfer('p', 'Pinning', props.downloadsWait || props.pins, props.pinProgress != null ? props.pinProgress : 0.3, props.downloadsWait === 'Paused') : null,
      props.makingLocal ? xfer('m', 'Moving to This Mac', props.downloadsWait || props.makingLocal, props.makeLocalProgress != null ? props.makeLocalProgress : 0.2, props.downloadsWait === 'Paused') : null].filter(Boolean))));
    // Under Transfers, Open Storage Manager for the detail.
    if (props.uploads || props.pins || props.makingLocal) out.push(h('div', { key: 'k', className: 'bae-m-actfoot' }, h(MacButton, { size: 'small' }, 'Open Storage Manager')));
    if (!out.length) out.push(h(React.Fragment, { key: 'i' }, sec(null, row('i', h('div', { className: 'bae-m-actline bae-muted' }, 'Everything is up to date.')))));
    return h('div', { className: 'bae-m-actlist' }, out);
  }



  // ---- Android: sync activity and Downloads ----
  // MacErrorDetails on Android: the panel with a copy icon button at its corner, in a dialog.
  function AndroidErrorDetails(props) {
    return h('div', { className: 'bae-errdis-panel bae-errdis-panel-android' }, h('div', { className: 'bae-errdis-chain' }, props.details), h('span', { className: 'bae-errdis-copy' }, h(AndroidIconButton, { icon: 'copy', label: 'Copy details', size: 16, box: 40, tone: 'muted' })));
  }
  // Android's notice: MacNotice's parts from Android's own, both buttons text buttons; centred, the primary is an outlined button. Details opens an AndroidDialog holding `AndroidErrorDetails`.
  function AndroidNotice(props) {
    var tone = props.tone || 'neutral';
    var centered = props.layout === 'centered';
    var primary = props.action ? h(AndroidButton, { key: 'p', kind: centered ? 'tonal' : 'text' }, props.action) : null;
    var secondary = props.secondary ? h(AndroidButton, { key: 's', kind: 'text' }, props.secondary) : null;
    var dismiss = props.dismiss ? h('span', { key: 'x', className: 'bae-a-notice-dismiss' }, h(AndroidInlineIconButton, { icon: 'close', label: 'Dismiss' })) : null;
    if (centered) {
      // The glyph sits beside the title only; a subtitle is its own smaller, grey line under them, so the two lines read as a heading and what to do.
      var title = h(React.Fragment, null,
        h('div', { className: 'bae-notice-centered-title' }, noticeGlyph(tone, props.icon, { centered: true }), h('div', { className: cx('bae-notice-title', props.subtitle && 'bae-notice-title-strong') }, props.title), props.children),
        props.subtitle ? h('div', { className: 'bae-notice-centered-sub' }, props.subtitle) : null);
      return NoticeBox(tone, props.icon, h('div', { className: 'bae-a-notice' }, title, primary || secondary ? h('div', { className: 'bae-notice-centered-row' }, secondary, primary) : null), { platform: 'bae-a-notice-box', quiet: props.quiet, flush: props.flush, centered: true, stage: props.stage });
    }
    // As on Mac: the buttons sit on the text's line at the right; `buttons: 'below'` puts them under the text.
    var below = props.buttons === 'below';
    var words = h('div', { key: 'w', className: 'bae-a-notice-words' }, noticeWords(props));
    var buttons = primary || secondary ? h('div', { key: 'b', className: below ? 'bae-a-notice-below' : 'bae-a-notice-buttons' }, secondary, primary) : null;
    // One top row, aligned to the title's line: the words, inline buttons, the ×; `below` buttons are a second row.
    var top = h('div', { className: 'bae-a-notice-top' }, words, below ? null : buttons, dismiss);
    var body = h('div', { className: 'bae-a-notice' }, top, below ? buttons : null);
    return NoticeBox(tone, props.icon, body, { platform: 'bae-a-notice-box', quiet: props.quiet, flush: props.flush, centered: false, stage: props.stage });
  }

  // The Activity sheet (SyncActivity.kt's, grown to hold every problem): the title, Needs Attention with the caller's notices (children), bae couldn't read sync's status (no button), and sync's own with Open Sync Settings; nothing retries in place; then the transfers (the downloads line opens Downloads), or up to date.
  function AndroidActivity(props) {
    var problems = props.children || props.error || props.attention;
    // Only the problems' rows start their words on the text edge, past the glyphs; the title, the Needs Attention label and the transfers sit at the sheet's edge.
    var t = function (s, c) { return h('div', { className: c || 'bae-a-act-line' }, s); };
    // `held` as on Mac: the pinning line ends in a tonal Pause, or Resume while paused. A phone never uploads release files, so Uploading has none.
    var xfer = function (key, label, count, progress, tap, held) {
      return h('div', { key: key, className: cx('bae-a-actxfer', tap && 'bae-a-act-tap') },
        h('div', { className: 'bae-a-actxfer-line' }, h('span', null, label), h('span', { className: 'bae-a-actxfer-end' }, count ? h('span', { className: 'bae-muted' }, count) : null,
          held != null ? h(AndroidIconButton, { icon: held ? 'play' : 'pause', size: 18, tonal: true, label: held ? 'Resume' : 'Pause' }) : null)),
        h(ProgressBar, { android: true, value: progress == null ? null : Number(progress), label: label }));
    };
    return h('div', { className: 'bae-a-act' },
      h('div', { className: 'bae-a-act-title' }, 'Activity'),
      // Each problem is a quiet notice, a line between rows.
      problems ? h('div', { className: 'bae-a-act-block' }, h(AndroidEyebrow, null, 'Needs Attention'),
        h('div', { className: 'bae-a-actproblems' }, divided(kids(props.children).concat([
          props.error ? h(AndroidNotice, { key: 'e', quiet: true, dismiss: true, tone: 'danger', title: props.error }) : null,
          props.attention ? h(AndroidNotice, { key: 'f', quiet: true, dismiss: true, tone: 'warning', title: props.attention, action: 'Open Sync Settings', buttons: 'below' }) : null,
          props.attention && props.record ? h('div', { key: 'r' }, h('div', { className: 'bae-strong' }, props.record), h(AndroidNotice, { quiet: true, tone: 'neutral', title: props.reason })) : null].filter(Boolean))))) : null,
      // As on Mac: Transfers is its own labelled group; each transfer is its words, how far it has got, and a bar; the pinning line opens the Pinning screen.
      props.syncing || props.uploads || props.pins ? h(AndroidEyebrow, null, 'Transfers') : null,
      props.syncing ? xfer('s', 'Syncing changes', null, null) : null,
      props.uploads ? xfer('u', 'Uploading', props.uploadsWait || props.uploads, props.uploadProgress != null ? props.uploadProgress : 0.4) : null,
      props.pins ? xfer('p', 'Pinning', props.downloadsWait || props.pins, props.pinProgress != null ? props.pinProgress : 0.3, true, props.downloadsWait === 'Paused') : null,
      !(problems || props.syncing || props.uploads || props.pins) ? t('Everything is up to date.', 'bae-a-act-sub') : null);
  }
  function AndroidSegmentedButtons(props) {
    var o = list(props.options);
    var seg = h('div', { className: 'bae-a-seg', role: 'radiogroup', 'aria-label': props.label || null }, o.map(function (x) { var on = x === String(props.selected); return h('span', { key: x, role: 'radio', 'aria-checked': on, className: cx('bae-a-seg-btn', on && 'bae-a-seg-on') }, on ? h(Glyph, { name: 'check', size: 18 }) : null, x); }));
    // `label`: what the choice is, in the secondary text over the buttons, as a form names it.
    return props.label ? h('div', { className: 'bae-a-seg-field' }, h('div', { className: 'bae-a-seg-label', 'aria-hidden': true }, props.label), seg) : seg;
  }

  // What bae keeps on this device, as Settings → Storage heads it: the total, one bar split by kind, and a legend line per kind with its size. `parts` is "Label:size:share,…" in the kinds' fixed order; each kind keeps its colour whatever its size or whichever kinds are shown: Pinned blue, Cache gold, Library orchid, From before the reset mint, anything else the neutral; in that order no two neighbours are hard to tell apart. The legend names every kind, so no colour stands alone.
  var USAGE_KIND = { 'Pinned': 1, 'Cache': 2, 'Library': 3, 'From before the reset': 4 };
  function storageUsage(props, p) {
    var parts = list(props.parts || 'Pinned:6.1 GB:0.72,Cache:1.8 GB:0.21,Images:420 MB:0.05,Library:180 MB:0.02').map(function (r) { var a = r.split(':'); return { label: a[0], size: a[1], share: Number(a[2]) }; });
    return h('div', { className: 'bae-usage bae-usage-' + p },
      h('div', { className: 'bae-usage-total' }, h('span', { className: 'bae-usage-num' }, props.total || '8.5 GB'), h('span', { className: 'bae-usage-of' }, props.of || (p === 'm' ? 'on this Mac' : 'on this phone'))),
      h('div', { className: 'bae-usage-bar', role: 'img', 'aria-label': parts.map(function (x) { return x.label + ' ' + x.size; }).join(', ') },
        parts.map(function (x, i) { return h('span', { key: i, className: 'bae-usage-seg bae-usage-seg-' + (USAGE_KIND[x.label] || 5), style: { flexGrow: x.share } }); })),
      h('div', { className: 'bae-usage-legend' }, parts.map(function (x, i) {
        return h('span', { key: i, className: 'bae-usage-key' }, h('span', { className: 'bae-usage-dot bae-usage-seg-' + (USAGE_KIND[x.label] || 5), 'aria-hidden': true }), h('span', null, x.label), h('span', { className: 'bae-usage-size' }, x.size));
      })));
  }
  function MacStorageUsage(props) { return storageUsage(props, 'm'); }
  function AndroidStorageUsage(props) { return storageUsage(props, 'a'); }
  // The Pinning screen's bar: Back, the title, Retry and Pause or Resume.
  function AndroidDownloadsBar(props) {
    return h('header', { className: 'bae-a-dlbar' }, h(AndroidIconButton, { icon: 'back', label: 'Back' }), h('span', { className: 'bae-a-dlbar-title' }, 'Pinning'),
      h(AndroidButton, { kind: 'text', off: !props.failed }, 'Retry'), h(AndroidButton, { kind: 'text', off: !!props.idle }, props.paused ? 'Resume' : 'Pause'));
  }
  // One entry of the Pinning screen: the release's name with Cancel (×) on its line, its files and size, then its state: Queued to pin, a bar with bytes, or Couldn’t pin with Details and Retry. A release since removed from the library is named in secondary text, so it doesn't read as a title.
  function AndroidDownloadRow(props) {
    var s = props.state || 'queued';
    var state = s === 'queued' ? h('div', { className: 'bae-a-librow-sub' }, 'Queued to pin')
      : s === 'active' ? h('div', null, h('div', { className: 'bae-a-dlbar-track' }, h('div', { style: { width: (Number(props.progress || 0.4) * 100) + '%' } })), h('div', { className: 'bae-a-fine bae-muted' }, props.bytes || '120 MB of 300 MB'))
      : h(AndroidNotice, { quiet: true, tone: 'danger', title: 'Could not pin', secondary: 'Details', action: 'Retry' });
    return h('div', { className: 'bae-a-dlrow2' },
      h('div', { className: 'bae-a-dlrow2-head' },
        h('div', { className: cx('bae-a-librow-title', !props.title && 'bae-a-dlrow2-gone') }, props.title || 'Release no longer in library'),
        h(AndroidInlineIconButton, { icon: 'close', label: 'Cancel' })),
      h('div', { className: 'bae-a-librow-sub' }, props.detail),
      state);
  }
