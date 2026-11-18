  // ==== part: import ====
  // ---- Import (macOS) ----
  // The left column: Found / Imported / Skipped, each one tab named with its count; the filter row: the Filter field, the narrowed count, then its buttons (the identifying ring, importing and scanning, each opening what it stands for, and the ⋯ menu with its filter dot); then the rows.
  function MacImportSidebar(props) {
    var tab = props.tab || 'Found';
    var counts = { Found: props.found || '0', Imported: props.imported || '0', Skipped: props.skipped || '0' };
    var ind = function (label, className, child, extra) { return h('button', Object.assign({ type: 'button', className: cx('bae-plainbtn', 'bae-m-isb-ind', className), 'aria-label': label, title: label }, extra), child); };
    return h('div', { className: 'bae-m-isb' },
      h('div', { className: 'bae-m-isb-tabs', role: 'tablist' }, ['Found', 'Imported', 'Skipped'].map(function (t) { var on = t === tab; return h('span', { key: t, role: 'tab', 'aria-selected': on, 'aria-label': t + ', ' + counts[t], className: cx('bae-m-isb-tab', on && 'bae-m-isb-tab-on') }, h('span', null, t), h(StatusChip, { tone: on ? 'accent' : 'neutral' }, counts[t])); })),
      h('div', { className: 'bae-divider' }),
      h('div', { className: 'bae-m-isb-filter' },
        h(Glyph, { set: 'mac', name: 'search', size: 12, className: 'bae-m-tertiary' }), bareInput({ label: 'Filter', placeholder: 'Filter', value: props.filter }),
        props.narrowed ? h('span', { className: 'bae-muted bae-num', style: { fontSize: 11.5 } }, props.narrowed) : null,
        props.identifying ? ind('Identifying', null, h('svg', { width: 14, height: 14, viewBox: '0 0 16 16', 'aria-hidden': true }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--hairline-strong)', strokeWidth: 2 }), h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--accent)', strokeWidth: 2, strokeDasharray: (Number(props.identifying) * 37.7) + ' 37.7', transform: 'rotate(-90 8 8)' }))) : null,
        props.importing ? ind('Importing…', 'bae-muted', h(Sym, { name: 'saveAs', size: 14 })) : null,
        props.scanning ? ind('Scanning folders', null, h(MacSpinner, { size: 12 })) : null,
        ind('Sorting, filtering and watched folders', 'bae-muted', [h(Glyph, { key: 'g', set: 'mac', name: 'ellipsis', size: 15 }), props.filtered ? h('span', { key: 'd', className: 'bae-m-isb-dot' }) : null], { 'aria-haspopup': 'menu' })),
      h('div', { className: 'bae-divider' }),
      props.empty ? h('div', { className: 'bae-m-isb-empty' }, h('div', { className: 'bae-m-cu-title' }, props.empty)) : h('div', { className: 'bae-m-isb-list' }, props.children));
  }
  // A group of folders, a button that folds the group: chevron and the folder's name.
  function MacImportGroupHeader(props) {
    return h('button', { type: 'button', className: 'bae-plainbtn bae-m-igh', 'aria-expanded': !props.closed }, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-m-tertiary', style: props.closed ? { transform: 'rotate(-90deg)' } : null }), h('span', { className: 'bae-ellipsis' }, props.name));
  }
  // A line of progress in the surrounding font: the phase, a bar filling the rest of the line, and an optional count. No `value` is a wait of no known length; `activity` fills the bar in the activity colour, for files moving to or from storage.
  function MacProgressLine(props) {
    var value = props.value == null || props.value === '' ? NaN : Number(props.value);
    return h('div', { className: 'bae-m-pline' },
      h('span', { className: 'bae-m-pline-label' }, props.label),
      h(ProgressBar, { value: isNaN(value) ? null : value, className: cx('bae-m-pline-bar', props.activity && 'bae-progress-activity'), label: props.label }),
      props.detail ? h('span', { className: 'bae-m-pline-detail' }, props.detail) : null);
  }
  // One candidate: cover, title over artist (or the folder's name), the state line, the trailing badge or wait mark. `error` is a skipped folder's reason, under the title and artist or the folder's name.
  function MacImportRow(props) {
    var trail = props.badge ? h(StatusChip, { tone: props.badgeTone || 'warning' }, props.badge)
      : props.state === 'identifyQueued' || props.state === 'importQueued' ? h('span', { className: 'bae-muted', title: props.state === 'importQueued' ? 'Waiting to import' : 'Waiting to be identified' }, h(Sym, { name: 'clock', size: 12 }))
      : props.state === 'identifying' ? h(MacSpinner, { size: 12 }) : null;
    var line1 = props.folder ? h('div', { className: 'bae-m-irow-folder' }, h(Sym, { name: 'folder', size: 13, className: 'bae-muted' }), h('span', { className: 'bae-mono bae-ellipsis', style: { fontSize: 12 } }, props.folder))
      : h('div', { className: 'bae-m-irow-t' }, h('span', { className: cx('bae-ellipsis', !props.title && 'bae-muted') }, props.title || 'Album title'), h('span', { className: 'bae-linkout', style: { opacity: props.record ? 1 : 0 } }, h(Sym, { name: 'external', size: 13, className: 'bae-muted' })));
    var line2 = props.state === 'importing' ? h('div', { className: 'bae-m-irow-step' }, h(MacProgressLine, { label: props.step || 'Reading files…', value: props.progress == null ? 0.4 : props.progress }))
      : props.state === 'uploading' ? h('div', { className: 'bae-m-irow-step' }, h(MacProgressLine, { label: 'Uploading', value: props.progress == null ? 0.4 : props.progress, activity: true }))
      : props.folder ? null : h('div', { className: 'bae-m-irow-sub bae-ellipsis' }, props.artist);
    var line3 = props.error ? h('div', { className: 'bae-m-irow-sub bae-ellipsis' }, props.error) : null;
    return h('div', { className: cx('bae-m-irow', props.grouped && 'bae-m-irow-grouped', props.selected && 'bae-m-irow-selected') },
      props.grouped ? h('span', { className: 'bae-m-irow-rail' }) : null,
      h(Artwork, { title: props.cover || props.title || props.folder, size: 44, radius: 6, missing: props.noCover }),
      h('div', { className: 'bae-m-irow-text' }, line1, line2, line3), h('span', { style: { flex: 'none', marginLeft: 'auto' } }, trail));
  }
  // Above the right pane: the tab chip, a button that goes to that tab; the folder glyph, a button that reveals the folder in Finder; the folder's name.
  function MacImportFolderLine(props) {
    return h('div', { className: 'bae-m-ifl' }, h('button', { type: 'button', className: 'bae-plainbtn bae-m-ifl-tab' }, h(StatusChip, { tone: 'accent' }, props.tab || 'Found')), h(Sym, { name: 'right', size: 10, className: 'bae-m-tertiary' }), h('button', { type: 'button', className: 'bae-plainbtn bae-m-ifl-reveal bae-muted', 'aria-label': 'Reveal in Finder', title: 'Reveal in Finder' }, h(Sym, { name: 'folder', size: 15 })), h('span', { className: 'bae-ellipsis bae-strong', style: { fontSize: 15 } }, props.folder));
  }
  // Where an import goes, one choice of two, as the person last chose it: This Mac or Cloud. One stored choice, shown wherever Import is (the draft card, the bulk pane's Import row) and in Settings → Import, so changing it in one place changes it in all.
  function MacImportDestination(props) { return h(MacPopUpButton, { label: 'Import to', value: props.to || 'Cloud', off: props.off }); }
  // A field drawn as the value it holds until the pointer is over it or it has focus, for the draft's values and a track's cells: `label` names it, `placeholder` shows while it is empty.
  function cellInput(label, value, placeholder, className) {
    return h('input', { type: 'text', className: cx('bae-m-icell', className), defaultValue: value || '', placeholder: placeholder, 'aria-label': label, spellCheck: false });
  }
  // The draft card: Identify with Automatic, Search and the Metadata menu (⋯); where the import goes and Import; the folder's check in a warning notice; the cover (a plain tile when it has none) and the identity: the album title and year as fields, the album artist as a button with its Library or New badge, the audio line, and the album pop-up; the Release grid of fields and pop-ups; the linked records with Unlink.
  function MacImportDraft(props) {
    var dash = function (v) { return v || '—'; };
    var grid = [
      ['Year', cellInput('Year', props.pressingYear, '—')],
      ['Media', h(MacPopUpButton, { label: 'Media', value: dash(props.media) })],
      ['Label', h('span', { className: 'bae-m-idraft-labelrow' }, cellInput('Label', props.labelName, '—'), cellInput('Catalog number', props.catalog, '—'))],
      ['Country', h(MacPopUpButton, { label: 'Country', value: dash(props.country) })],
      ['Barcode', cellInput('Barcode', props.barcode, '—')],
      ['Status', h(MacPopUpButton, { label: 'Status', value: dash(props.status) })],
      ['Packaging', h(MacPopUpButton, { label: 'Packaging', value: dash(props.packaging) })],
      ['Details', cellInput('Details', props.details, '—')]];
    return h('div', { className: 'bae-m-card bae-m-idraft' },
      h('div', { className: 'bae-m-idraft-actions' },
        h('span', { className: 'bae-strong bae-muted' }, 'Identify'), h(MacButton, null, 'Automatic'), h(MacButton, null, 'Search'), props.editor ? null : h(MacButton, { menu: true, label: 'Metadata' }, '⋯'),
        h('span', { style: { flex: 1 } }),
        props.cloud ? h(MacImportDestination, { to: props.to }) : null,
        props.editor ? [props.changed ? h(MacButton, { key: 'c' }, 'Cancel') : null, h(MacButton, { key: 's', kind: 'primary' }, 'Save')] : props.blank ? null : h(MacButton, { kind: 'primary' }, props.retry ? 'Retry import' : 'Import')),
      props.check ? h(MacNotice, { tone: 'warning', title: props.check }) : null,
      h('div', { className: 'bae-m-idraft-head' },
        h(Artwork, { title: props.cover, size: 132, radius: 6, missing: !props.cover }),
        h('div', { className: 'bae-m-idraft-id' },
          cellInput('Album title', props.title, 'Album title', 'bae-m-idraft-title'),
          h('button', { type: 'button', className: 'bae-plainbtn bae-m-idraft-artist' }, h('span', { className: props.artist ? 'bae-muted' : 'bae-m-tertiary' }, props.artist || 'Album artist'), props.artistChip ? h(StatusChip, { tone: props.artistChip === 'New' ? 'accent' : 'neutral' }, props.artistChip) : null),
          cellInput('Album year', props.year, 'Album year', 'bae-m-idraft-year'),
          // The folder's audio as the scan read it (spec §3.6): codec, sample rate, bit depth or bitrate, channels.
          props.audio ? h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.audio) : null,
          // The album the release files under (spec §3.7 step 6, §3.8): New album, the library album its records name, or one picked; the pop-up is the album picker.
          props.album ? h('div', { className: 'bae-m-idraft-album' }, h('span', { className: 'bae-muted' }, 'Album'), h(MacPopUpButton, { value: props.album, label: 'Album', open: props.albumOpen, items: props.albumItems })) : null)),
      h('div', { className: 'bae-m-formhead' }, 'Release'),
      h('div', { className: 'bae-m-idraft-grid' }, grid.map(function (r, i) { return [h('span', { key: 'l' + i, className: 'bae-muted' }, r[0]), h('span', { key: 'v' + i, className: 'bae-m-idraft-cell' }, r[1])]; })),
      props.records ? h('div', { className: 'bae-m-idraft-records' }, h(MacRecordLinks, { records: props.records }), h('span', { style: { flex: 1 } }), h('button', { type: 'button', className: 'bae-plainbtn bae-accent' }, 'Unlink')) : null);
  }
  // The folder's images under the release: a ruled Images heading, then each image at 128pt, a button named by its file that opens the lightbox at it (`images`: "artwork title|file name", comma-separated).
  function MacImportImages(props) {
    return h('div', { className: 'bae-m-ifiles' }, h('div', { className: 'bae-m-formhead' }, 'Images'),
      h('div', { className: 'bae-m-iimages' }, list(props.images).map(function (t) { var p = t.split('|'); return h('button', { key: t, type: 'button', className: 'bae-plainbtn bae-m-iimage', 'aria-label': p[1] || p[0] }, h(Artwork, { title: p[0], size: 128, radius: 6 })); })));
  }
  // The release's files that are no track (a rip log, a cover image): a ruled Files heading, then one line per file, its name in mono and its size at the right. A document, a track sheet or an image is a button named by its file, which opens it; `other` marks a file that opens nothing (`files`: "name|size[|other]", comma-separated).
  function MacReleaseFiles(props) {
    return h('div', { className: 'bae-m-ifiles' }, h('div', { className: 'bae-m-formhead' }, 'Files'),
      h('div', null, list(props.files).map(function (f) {
        var p = f.split('|'), other = p[2] === 'other';
        return h(other ? 'div' : 'button', { key: p[0], type: other ? null : 'button', className: cx(!other && 'bae-plainbtn', 'bae-m-ifile'), 'aria-label': other ? null : p[0] }, h('span', { className: 'bae-mono bae-ellipsis' }, p[0]), h('span', { className: 'bae-m-tertiary bae-m-ifile-size' }, p[1] || ''));
      })));
  }
  // The mapping table: Source | Track | Title | Artist | Length, a side's eyebrow, CUE captions and rows.
  function MacImportTable(props) {
    return h('div', { className: 'bae-m-itable' }, h('div', { className: 'bae-m-itable-row bae-m-itable-head' }, ['Source', 'Track', 'Title', 'Artist', 'Length'].map(function (t) { return h('span', { key: t, className: 'bae-m-eyebrowtxt' }, t); })), props.children);
  }
  // `start`: a CUE image's track, its source cell where the track starts in the image (the sheet's caption names the image once). `listed`: the length the source lists, after the measured `length` in the warning colour, when the two are over 3 s apart, so the row says what disagrees. The title and the artist are fields; an artist the track inherits from the album leaves its field empty, showing Album artist.
  function MacImportTrackRow(props) {
    return h('div', { className: cx('bae-m-itable-row', 'bae-m-itable-track', props.playing && 'bae-m-itable-playing') },
      h('span', { className: 'bae-m-isrc' }, h('button', { type: 'button', className: cx('bae-plainbtn', props.playing ? 'bae-accent' : 'bae-muted'), 'aria-label': props.playing ? 'Pause' : 'Play' }, h(Sym, { name: props.playing ? 'pause' : 'play', size: 10 })), props.start != null ? h('span', { className: 'bae-num bae-muted' }, props.start) : h('span', { className: 'bae-mono bae-ellipsis' }, props.file)),
      h('span', { className: 'bae-num' }, props.position),
      h('span', { className: 'bae-m-itable-cell' }, cellInput('Title', props.title, 'Untitled')),
      h('span', { className: 'bae-m-itable-cell' }, cellInput('Artist', props.inherited ? '' : props.artist, 'Album artist')),
      h('span', { className: 'bae-num', style: { textAlign: 'right' } }, props.length, props.listed ? h('span', { className: 'bae-warning', title: 'Listed length' }, ' ' + props.listed) : null));
  }
  // A CUE sheet's caption: the pop-up of which disc the sheet is, CUE, the sheet's name (a button that opens it in the viewer), → and the pop-up of the audio its FILE line is bound to.
  function MacImportSheetCaption(props) {
    return h('div', { className: 'bae-m-icue' },
      h('button', { type: 'button', role: 'combobox', 'aria-label': 'Disc', 'aria-expanded': false, className: 'bae-plainbtn bae-m-icue-disc' }, h('span', { className: 'bae-strong' }, props.disc), h(Glyph, { set: 'mac', name: 'down', size: 8 })),
      h(StatusChip, { tone: 'accent' }, 'CUE'),
      h('button', { type: 'button', className: 'bae-plainbtn bae-mono' }, props.sheet),
      h('span', { className: 'bae-m-tertiary', 'aria-hidden': true }, '→'),
      h('button', { type: 'button', role: 'combobox', 'aria-label': 'Audio file', 'aria-expanded': false, className: cx('bae-plainbtn', 'bae-mono', props.unbound ? 'bae-warning' : 'bae-muted') }, props.bound || 'Choose audio…'));
  }
  // Find online: Back (a button) and Identify, the Discogs bar, then Automatic and Search, each heading a button that opens its section, one open at a time.
  function MacFindOnline(props) {
    var open = props.open || 'Automatic';
    var head = function (t, status) { return h('button', { type: 'button', 'aria-expanded': open === t, className: cx('bae-plainbtn', 'bae-m-fo-sec', open !== t && props.dimmed === t && 'bae-m-fo-dim') }, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-m-tertiary', style: open === t ? null : { transform: 'rotate(-90deg)' } }), h('span', { className: 'bae-m-eyebrowtxt' }, t), status); };
    var k = kids(props.children);
    return h('div', { className: 'bae-m-card bae-m-fo' },
      h('div', { className: 'bae-m-fo-head' }, h('button', { type: 'button', className: 'bae-plainbtn bae-accent bae-m-fo-back' }, h(Sym, { name: 'left', size: 12 }), 'Back'), h('span', { className: 'bae-strong', style: { position: 'absolute', left: 0, right: 0, textAlign: 'center', pointerEvents: 'none' } }, 'Identify')),
      h('div', { className: 'bae-divider' }),
      props.discogsBar ? h('div', { style: { padding: 12 } }, h(MacNotice, { tone: 'info' }, h('div', { style: { display: 'flex', gap: 10 } }, h('div', { style: { flex: 1 } }, h('div', { className: 'bae-strong' }, 'Add Discogs to find more pressings'), h('div', { className: 'bae-muted' }, 'Many regional releases, reissues and vinyl pressings are only on Discogs. Tokens are free: create one in your Discogs account settings.')), h(MacButton, { kind: 'primary', size: 'small' }, 'Open Settings'), h('button', { type: 'button', className: 'bae-plainbtn bae-m-fo-close', 'aria-label': 'Close' }, h(Sym, { name: 'close', size: 11 }))))) : null,
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
      return h('span', { key: i, className: cx('bae-m-idchip', 'bae-m-idchip-' + st) }, h('span', { className: 'bae-m-tertiary', style: { fontSize: 10.5, fontWeight: 600 } }, lab), st === 'dash' ? h('span', { className: 'bae-m-idchip-dashmark' }) : h('span', { className: 'bae-mono' }, val),
        cap ? cap.split('+').map(function (cc, j) { var q = cc.split('='); return h('span', { key: j, className: 'bae-m-capsule' }, q[0], q[1] === '…' ? h('span', { className: 'bae-m-tertiary' }, 'Searching…') : h(StatusChip, { tone: Number(q[1]) > 0 ? 'success' : 'neutral' }, q[1])); }) : null);
    }));
  }
  // Every catalog that describes a release, as a wrapped row of links to each catalog's page, in the order core lists them (`records`: catalog names, comma-separated).
  function MacRecordLinks(props) {
    return h('span', { className: 'bae-m-reclinks' }, list(props.records).map(function (s) { return h('a', { key: s, href: '#', className: 'bae-m-reclink' }, s, h('span', { className: 'bae-linkout' }, h(Sym, { name: 'external', size: 12 }))); }));
  }
  // A found album: cover, title, artist · label, a link per catalog that has answered (`sources`); its pressings under it on a rule.
  function MacReleaseCard(props) {
    return h('div', { className: 'bae-m-rcard' },
      h('div', { className: 'bae-m-rcard-head' }, h(Artwork, { title: props.title, size: 44, radius: 6 }), h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.title), h('div', { className: 'bae-muted' }, props.artist + (props.label ? ' · ' + props.label : ''))), props.sources ? h(MacRecordLinks, { records: props.sources }) : null),
      h('div', { className: 'bae-m-rcard-rows' }, props.children));
  }
  // A fact the pressing's records state differently is every record's value, each followed by its catalog's name: "1992 MusicBrainz / 1993 Discogs". Written "1992:MusicBrainz|1993:Discogs"; in `summary`, one of its " · " parts. A fact one record states alone is shown as stated.
  function pressingValues(text) {
    if (String(text).indexOf('|') < 0) return text;
    return String(text).split('|').map(function (v, i) {
      var at = v.lastIndexOf(':');
      return h(React.Fragment, { key: i }, i ? h('span', { className: 'bae-m-tertiary' }, ' / ') : null, v.slice(0, at), h('span', { className: 'bae-m-tertiary bae-m-prow-catalog' }, ' ' + v.slice(at + 1)));
    });
  }
  function MacPressingRow(props) {
    var summary = props.summary ? String(props.summary).split(' · ').map(function (part, i) { return h(React.Fragment, { key: i }, i ? ' · ' : null, pressingValues(part)); }) : null;
    return h('div', { className: cx('bae-m-prow', props.selected && 'bae-m-prow-selected'), style: props.inLibrary ? { opacity: 0.55 } : null },
      h('div', { style: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 2 } },
        h('div', { style: { display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap' } }, props.year ? h('span', { className: 'bae-strong bae-num' }, pressingValues(props.year)) : h('span', { className: 'bae-m-tertiary' }, 'Year unknown'), h('span', { className: 'bae-muted' }, props.labels), props.catalog ? h(StatusChip, null, pressingValues(props.catalog)) : null, props.barcode ? h('span', { className: 'bae-muted bae-num' }, pressingValues(props.barcode)) : null, list(props.badges).map(function (b) { return h(StatusChip, { key: b, tone: 'accent' }, b); })),
        summary ? h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, summary) : null,
        // `note`: the records' notes (a release's disambiguation), every record's that states one, each with its catalog when they differ.
        props.note ? h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5, fontStyle: 'italic' } }, pressingValues(props.note)) : null),
      props.inLibrary ? h('span', { style: { display: 'flex', alignItems: 'center', gap: 4, fontSize: 11.5 }, className: 'bae-m-tertiary' }, h(Sym, { name: 'check', size: 11, className: 'bae-success' }), 'In library') : null,
      // `sources`: the catalogs whose records the pressing holds, only those that have answered.
      props.sources ? h('span', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.sources) : null,
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
        grp('Import', [row('saveAs', 'Import', props.ready || 3, true, props.cloud ? h(MacImportDestination, { to: props.to }) : null), row('xcircle', 'Cancel import of selected', 0), row('grid', 'Combine as one release', 0, true), row('grid', 'Keep as separate releases', 0)]),
        grp('Metadata', [row('refresh', 'Identify selected', n), row('xcircle', 'Stop identifying selected', 0), row('refresh', 'Retry failed identification', 1), row('edit', 'Reset to file metadata', n), row('trash', 'Clear metadata', n)]),
        grp(null, [row('xcircle', 'Skip selected', n), row('left', 'Restore to Pending', 0)]),
        grp(null, [row('folder', 'Reveal in Finder', n)]))),
      props.running ? h('div', { style: { width: 440, display: 'flex', gap: 12, alignItems: 'flex-end' } }, h('div', { style: { flex: 1 } }, h(MacProgress, { label: props.running, value: 0.4 })), h(MacButton, null, 'Cancel')) : null);
  }
