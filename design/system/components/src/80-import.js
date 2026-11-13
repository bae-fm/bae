  // ==== part: import ====
  // ---- Import (macOS) ----
  // The left column: Found / Imported / Skipped with counts, the filter row with its indicators, then the rows.
  function MacImportSidebar(props) {
    var tab = props.tab || 'Found';
    var counts = { Found: props.found || '0', Imported: props.imported || '0', Skipped: props.skipped || '0' };
    return h('div', { className: 'bae-m-isb' },
      h('div', { className: 'bae-m-isb-tabs' }, ['Found', 'Imported', 'Skipped'].map(function (t) { var on = t === tab; return h('span', { key: t, className: cx('bae-m-isb-tab', on && 'bae-m-isb-tab-on') }, h('span', null, t), h(StatusChip, { tone: on ? 'accent' : 'neutral' }, counts[t])); })),
      h('div', { className: 'bae-divider' }),
      h('div', { className: 'bae-m-isb-filter' },
        h(Glyph, { set: 'mac', name: 'search', size: 12, className: 'bae-m-tertiary' }), h('span', { className: props.filter ? null : 'bae-m-tertiary', style: { flex: 1 } }, props.filter || 'Filter…'),
        props.narrowed ? h('span', { className: 'bae-muted bae-num', style: { fontSize: 11.5 } }, props.narrowed) : null,
        props.identifying ? h('span', { className: 'bae-m-isb-ind', title: 'Identifying' }, h('svg', { width: 14, height: 14, viewBox: '0 0 16 16' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--hairline-strong)', strokeWidth: 2 }), h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--accent)', strokeWidth: 2, strokeDasharray: (Number(props.identifying) * 37.7) + ' 37.7', transform: 'rotate(-90 8 8)' }))) : null,
        props.importing ? h('span', { className: 'bae-m-isb-ind bae-muted', title: 'Importing…' }, h(Sym, { name: 'saveAs', size: 14 })) : null,
        props.scanning ? h('span', { className: 'bae-m-isb-ind', title: 'Scanning folders' }, h(MacSpinner, { size: 12 })) : null,
        h('span', { className: 'bae-m-isb-ind bae-muted', title: 'Sorting, Filtering and Watched Folders' }, h(Glyph, { set: 'mac', name: 'ellipsis', size: 15 }), props.filtered ? h('span', { className: 'bae-m-isb-dot' }) : null)),
      h('div', { className: 'bae-divider' }),
      props.empty ? h('div', { className: 'bae-m-isb-empty' }, h('div', { className: 'bae-m-cu-title' }, props.empty)) : h('div', { className: 'bae-m-isb-list' }, props.children));
  }
  function MacImportGroupHeader(props) {
    return h('div', { className: 'bae-m-igh' }, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-m-tertiary', style: props.closed ? { transform: 'rotate(-90deg)' } : null }), h('span', { className: 'bae-ellipsis' }, props.name));
  }
  // A line of progress in the surrounding font: the phase, a bar filling the rest of the line, and an optional count. No `value` is a wait of no known length; `activity` fills the bar in the activity colour, for files moving to or from storage.
  function MacProgressLine(props) {
    var value = props.value == null || props.value === '' ? NaN : Number(props.value);
    return h('div', { className: 'bae-m-pline' },
      h('span', { className: 'bae-m-pline-label' }, props.label),
      h(ProgressBar, { value: isNaN(value) ? null : value, className: cx('bae-m-pline-bar', props.activity && 'bae-progress-activity'), label: props.label }),
      props.detail ? h('span', { className: 'bae-m-pline-detail' }, props.detail) : null);
  }
  // One candidate: cover, title over artist (or the folder's name), the state line, the trailing badge or wait mark.
  function MacImportRow(props) {
    var trail = props.badge ? h(StatusChip, { tone: props.badgeTone || 'warning' }, props.badge)
      : props.state === 'identifyQueued' || props.state === 'importQueued' ? h('span', { className: 'bae-muted', title: props.state === 'importQueued' ? 'Waiting to import' : 'Waiting to be identified' }, h(Sym, { name: 'clock', size: 12 }))
      : props.state === 'identifying' ? h(MacSpinner, { size: 12 }) : null;
    var line1 = props.folder ? h('div', { className: 'bae-m-irow-folder' }, h(Sym, { name: 'folder', size: 13, className: 'bae-muted' }), h('span', { className: 'bae-mono bae-ellipsis', style: { fontSize: 12 } }, props.folder))
      : h('div', { className: 'bae-m-irow-t' }, h('span', { className: cx('bae-ellipsis', !props.title && 'bae-muted') }, props.title || 'Album title'), h('span', { className: 'bae-linkout', style: { opacity: props.record ? 1 : 0 } }, h(Sym, { name: 'external', size: 13, className: 'bae-muted' })));
    var line2 = props.state === 'importing' ? h('div', { className: 'bae-m-irow-step' }, h(MacProgressLine, { label: props.step || 'Reading files…', value: props.progress == null ? 0.4 : props.progress }))
      : props.state === 'uploading' ? h('div', { className: 'bae-m-irow-step' }, h(MacProgressLine, { label: 'Uploading', value: props.progress == null ? 0.4 : props.progress, activity: true }))
      : props.error ? h('div', { className: 'bae-m-irow-sub bae-ellipsis' }, props.error)
      : props.folder ? null : h('div', { className: 'bae-m-irow-sub bae-ellipsis' }, props.artist);
    return h('div', { className: cx('bae-m-irow', props.grouped && 'bae-m-irow-grouped', props.selected && 'bae-m-irow-selected') },
      props.grouped ? h('span', { className: 'bae-m-irow-rail' }) : null,
      h(Artwork, { title: props.cover || props.title || props.folder, size: 44, radius: 6, missing: props.noCover }),
      h('div', { className: 'bae-m-irow-text' }, line1, line2), h('span', { style: { flex: 'none', marginLeft: 'auto' } }, trail));
  }
  // Above the right pane: the tab chip, Reveal in Finder, the folder's name.
  function MacImportFolderLine(props) {
    return h('div', { className: 'bae-m-ifl' }, h(StatusChip, { tone: 'accent' }, props.tab || 'Found'), h(Sym, { name: 'right', size: 10, className: 'bae-m-tertiary' }), h(Sym, { name: 'folder', size: 15, className: 'bae-muted' }), h('span', { className: 'bae-ellipsis bae-strong', style: { fontSize: 15 } }, props.folder));
  }
  // Where an import goes, one choice of two, as the person last chose it: This Mac or Cloud. One stored choice, shown wherever Import is (the draft card, the bulk pane's Import row) and in Settings → Import, so changing it in one place changes it in all.
  function MacImportDestination(props) { return h(MacPopUpButton, { value: props.to || 'Cloud', off: props.off }); }
  // The draft card: Identify with Automatic, Search and ⋯; where the import goes and Import; the folder's check in a warning notice; the cover well and identity; the Release grid; the linked records.
  function MacImportDraft(props) {
    var rows = list(props.fields || 'Year:1992,Media:CD,Label:Harbor Records — HR-1042,Country:—,Barcode:—,Status:Official,Packaging:—,Details:—').map(function (r) { var i = r.indexOf(':'); return [r.slice(0, i), r.slice(i + 1)]; });
    return h('div', { className: 'bae-m-card bae-m-idraft' },
      h('div', { className: 'bae-m-idraft-actions' },
        h('span', { className: 'bae-strong bae-muted' }, 'Identify'), h(MacButton, null, 'Automatic'), h(MacButton, null, 'Search'), props.editor ? null : h(MacButton, null, '⋯'),
        h('span', { style: { flex: 1 } }),
        props.cloud ? h(MacImportDestination, { to: props.to }) : null,
        props.editor ? [props.changed ? h(MacButton, { key: 'c' }, 'Cancel') : null, h(MacButton, { key: 's', kind: 'primary' }, 'Save')] : props.blank ? null : h(MacButton, { kind: 'primary' }, props.retry ? 'Retry Import' : 'Import')),
      props.check ? h(MacNotice, { tone: 'warning', title: props.check }) : null,
      h('div', { className: 'bae-m-idraft-head' },
        props.cover ? h(Artwork, { title: props.cover, size: 132, radius: 6 }) : h('div', { className: 'bae-m-idraft-well' }, h('div', { className: 'bae-strong' }, 'Add artwork'), h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, 'Drag an image here, or click to choose')),
        h('div', { className: 'bae-m-idraft-id' },
          h('div', { className: cx('bae-m-idraft-title', !props.title && 'bae-m-tertiary') }, props.title || 'Album title'),
          h('div', { className: 'bae-muted', style: { display: 'flex', gap: 6, alignItems: 'center' } }, props.artist || 'Album artist', props.artistChip ? h(StatusChip, { tone: props.artistChip === 'New' ? 'accent' : 'neutral' }, props.artistChip) : null),
          h('div', { className: 'bae-m-tertiary' }, props.year || 'Album year'),
          h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.audio || 'FLAC · 16-bit · 44.1 kHz · 10 files'))),
      h('div', { className: 'bae-m-formhead' }, 'Release'),
      h('div', { className: 'bae-m-idraft-grid' }, rows.map(function (r, i) { return [h('span', { key: 'l' + i, className: 'bae-muted' }, r[0]), h('span', { key: 'v' + i, className: r[1] === '—' ? 'bae-m-tertiary' : null }, r[1])]; })),
      props.records ? h('div', { className: 'bae-m-idraft-records' }, h(MacRecordLinks, { records: props.records }), h('span', { style: { flex: 1 } }), h('span', { className: 'bae-accent' }, 'Unlink')) : null);
  }
  // The mapping table: Source | Track | Title | Artist | Length, a side's eyebrow, CUE captions and rows.
  // The folder's images under the release: a ruled Images heading, then each image at 128pt (`images`: artwork titles, comma-separated).
  function MacImportImages(props) {
    return h('div', { className: 'bae-m-ifiles' }, h('div', { className: 'bae-m-formhead' }, 'Images'),
      h('div', { className: 'bae-m-iimages' }, list(props.images).map(function (t) { return h(Artwork, { key: t, title: t, size: 128, radius: 6 }); })));
  }
  // The release's files that are no track (a rip log, a cover image): a ruled Files heading, then one line per file, its name in mono and its size at the right (`files`: "name|size", comma-separated).
  function MacReleaseFiles(props) {
    return h('div', { className: 'bae-m-ifiles' }, h('div', { className: 'bae-m-formhead' }, 'Files'),
      h('div', null, list(props.files).map(function (f) { var p = f.split('|'); return h('div', { key: p[0], className: 'bae-m-ifile' }, h('span', { className: 'bae-mono bae-ellipsis' }, p[0]), h('span', { className: 'bae-m-tertiary bae-m-ifile-size' }, p[1] || '')); })));
  }
  function MacImportTable(props) {
    return h('div', { className: 'bae-m-itable' }, h('div', { className: 'bae-m-itable-row bae-m-itable-head' }, ['Source', 'Track', 'Title', 'Artist', 'Length'].map(function (t) { return h('span', { key: t, className: 'bae-m-eyebrowtxt' }, t); })), props.children);
  }
  function MacImportTrackRow(props) {
    return h('div', { className: cx('bae-m-itable-row', 'bae-m-itable-track', props.playing && 'bae-m-itable-playing') },
      h('span', { className: 'bae-m-isrc' }, h('span', { className: props.playing ? 'bae-accent' : 'bae-muted' }, h(Sym, { name: props.playing ? 'pause' : 'play', size: 10 })), h('span', { className: props.entry ? 'bae-ellipsis' : 'bae-mono bae-ellipsis' }, props.file)),
      h('span', { className: 'bae-num' }, props.position),
      h('span', { className: cx('bae-ellipsis', !props.title && 'bae-m-tertiary') }, props.title || 'Untitled'),
      h('span', { className: cx('bae-ellipsis', props.inherited && 'bae-m-tertiary') }, props.artist || 'Album artist'),
      h('span', { className: cx('bae-num', props.mismatch && 'bae-warning'), style: { textAlign: 'right' }, title: props.mismatch ? 'Lengths differ' : null }, props.length));
  }
  function MacImportSheetCaption(props) {
    return h('div', { className: 'bae-m-icue' }, h('span', { className: 'bae-m-icue-disc' }, h('span', { className: 'bae-strong' }, props.disc || 'Disc 1'), h(Glyph, { set: 'mac', name: 'down', size: 8 })), h(StatusChip, { tone: 'accent' }, 'CUE'), h('span', { className: 'bae-mono' }, props.sheet), h('span', { className: 'bae-m-tertiary' }, '→'), h('span', { className: cx('bae-mono', props.unbound ? 'bae-warning' : 'bae-muted') }, props.bound || 'Choose audio…'));
  }
  // Find online: Back and Identify, the Discogs bar, then Automatic and Search, one open at a time.
  function MacFindOnline(props) {
    var open = props.open || 'Automatic';
    var head = function (t, status) { return h('div', { className: cx('bae-m-fo-sec', open !== t && props.dimmed === t && 'bae-m-fo-dim') }, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-m-tertiary', style: open === t ? null : { transform: 'rotate(-90deg)' } }), h('span', { className: 'bae-m-eyebrowtxt' }, t), status); };
    var k = kids(props.children);
    return h('div', { className: 'bae-m-card bae-m-fo' },
      h('div', { className: 'bae-m-fo-head' }, h('span', { className: 'bae-accent', style: { display: 'flex', alignItems: 'center', gap: 4 } }, h(Sym, { name: 'left', size: 12 }), 'Back'), h('span', { className: 'bae-strong', style: { position: 'absolute', left: 0, right: 0, textAlign: 'center', pointerEvents: 'none' } }, 'Identify')),
      h('div', { className: 'bae-divider' }),
      props.discogsBar ? h('div', { style: { padding: 12 } }, h(MacNotice, { tone: 'info' }, h('div', { style: { display: 'flex', gap: 10 } }, h('div', { style: { flex: 1 } }, h('div', { className: 'bae-strong' }, 'Add Discogs to find more pressings'), h('div', { className: 'bae-muted' }, 'Many regional releases, reissues and vinyl pressings are only on Discogs. Tokens are free: create one in your Discogs account settings.')), h(MacButton, { kind: 'primary', size: 'small' }, 'Open Settings'), h(Sym, { name: 'close', size: 11 })))) : null,
      head('Automatic', props.autoStatus === 'working' ? h(MacSpinner, { size: 11 }) : props.autoStatus === 'failed' ? h(StatusGlyph, { tone: 'warning', set: 'mac', size: 10 }) : null),
      open === 'Automatic' ? h('div', { className: 'bae-m-fo-body' }, k) : null,
      h('div', { className: 'bae-divider' }),
      head('Search', null),
      open === 'Search' ? h('div', { className: 'bae-m-fo-body' }, k) : null);
  }
  // The identifier chips: "Label:value[:state]" with state filled, outlined, struck or dash; capsules "Source=count".
  function MacIdentifierBand(props) {
    return h('div', { className: 'bae-m-band' }, list(props.chips).map(function (c, i) {
      var p = c.split('|'); var lab = p[0], val = p[1] || '', st = p[2] || 'filled', cap = p[3];
      return h('span', { key: i, className: cx('bae-m-idchip', 'bae-m-idchip-' + st) }, h('span', { className: 'bae-m-tertiary', style: { fontSize: 10.5, fontWeight: 600 } }, lab), st === 'dash' ? h('span', { className: 'bae-m-idchip-dash' }) : h('span', { className: 'bae-mono' }, val),
        cap ? cap.split('+').map(function (cc, j) { var q = cc.split('='); return h('span', { key: j, className: 'bae-m-capsule' }, q[0], q[1] === '…' ? h('span', { className: 'bae-m-tertiary' }, 'Searching…') : h(StatusChip, { tone: Number(q[1]) > 0 ? 'success' : 'neutral' }, q[1])); }) : null);
    }));
  }
  // Every catalog that describes a release, as a wrapped row of links to each catalog's page, in the order core lists them (`records`: catalog names, comma-separated).
  function MacRecordLinks(props) {
    return h('span', { className: 'bae-m-reclinks' }, list(props.records).map(function (s) { return h('span', { key: s, className: 'bae-m-reclink' }, s, h('span', { className: 'bae-linkout' }, h(Sym, { name: 'external', size: 12 }))); }));
  }
  function MacReleaseCard(props) {
    return h('div', { className: 'bae-m-rcard' },
      h('div', { className: 'bae-m-rcard-head' }, h(Artwork, { title: props.title, size: 44, radius: 6 }), h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.title), h('div', { className: 'bae-muted' }, props.artist + (props.label ? ' · ' + props.label : ''))), h(MacRecordLinks, { records: props.sources || 'MusicBrainz,Discogs' })),
      h('div', { className: 'bae-m-rcard-rows' }, props.children));
  }
  function MacPressingRow(props) {
    return h('div', { className: cx('bae-m-prow', props.selected && 'bae-m-prow-selected'), style: props.inLibrary ? { opacity: 0.55 } : null },
      h('div', { style: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 2 } },
        h('div', { style: { display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap' } }, props.year ? h('span', { className: 'bae-strong bae-num' }, props.year) : h('span', { className: 'bae-m-tertiary' }, 'Year unknown'), h('span', { className: 'bae-muted' }, props.labels), props.catalog ? h(StatusChip, null, props.catalog) : null, list(props.badges).map(function (b) { return h(StatusChip, { key: b, tone: 'accent' }, b); })),
        props.summary ? h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.summary) : null),
      h('span', { style: { display: 'flex', alignItems: 'center', gap: 4, opacity: props.inLibrary ? 1 : 0, fontSize: 11.5 }, className: 'bae-m-tertiary' }, h(Sym, { name: 'check', size: 11, className: 'bae-success' }), 'In library'),
      h('span', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.sources || 'MusicBrainz · Discogs'),
      h(Sym, { name: 'right', size: 11, className: props.selected ? 'bae-accent' : 'bae-m-tertiary' }));
  }
  // Several rows selected: the count, then the actions in groups, each with how many it applies to.
  function MacImportBulk(props) {
    // `extra` sits on the row before its count: Import's destination, on Import's own line.
    var row = function (icon, label, n, strong, extra) { return h('div', { key: label, className: cx('bae-m-bulkrow', !n && 'bae-m-bulkrow-off') }, h(Sym, { name: icon, size: 15, className: strong ? 'bae-accent' : 'bae-muted' }), h('span', { className: strong ? 'bae-strong' : 'bae-muted', style: { flex: 1 } }, label), extra || null, h(StatusChip, { tone: strong ? 'accent' : 'neutral' }, String(n || 0))); };
    var grp = function (eyebrow, rows) { return h('div', { className: 'bae-m-bulkgrp' }, eyebrow ? h('div', { className: 'bae-m-eyebrowtxt', style: { padding: '8px 12px 2px' } }, eyebrow) : null, rows); };
    var n = Number(props.count || 5);
    return h('div', { className: 'bae-m-bulk' }, h('div', { className: 'bae-m-card', style: { width: 440, padding: 24, display: 'flex', flexDirection: 'column', gap: 12 } },
      h('div', { style: { font: '700 22px var(--font-sans)' } }, n + ' selected'), h('div', { className: 'bae-muted' }, 'Each action applies only to eligible selected folders.'),
      h('div', { className: 'bae-m-bulklist' },
        grp('Import', [row('saveAs', 'Import', props.ready || 3, true, props.cloud ? h(MacImportDestination, { to: props.to }) : null), row('xcircle', 'Cancel import of selected', 0), row('grid', 'Combine as One Release', 0, true), row('grid', 'Keep as Separate Releases', 0)]),
        grp('Metadata', [row('refresh', 'Identify selected', n), row('xcircle', 'Stop identifying selected', 0), row('refresh', 'Retry failed identification', 1), row('edit', 'Reset to file metadata', n), row('trash', 'Clear metadata', n)]),
        grp(null, [row('xcircle', 'Skip selected', n), row('left', 'Restore to Pending', 0)]),
        grp(null, [row('folder', 'Reveal in Finder', n)]))),
      props.running ? h('div', { style: { width: 440, display: 'flex', gap: 12, alignItems: 'flex-end' } }, h('div', { style: { flex: 1 } }, h(MacProgress, { label: props.running, value: 0.4 })), h(MacButton, null, 'Cancel')) : null);
  }
