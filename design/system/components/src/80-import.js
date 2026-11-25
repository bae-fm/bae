  // ==== part: import ====
  // ---- Import (macOS) ----
  // A search or filter field: the magnifier, in a square its icon size wide as the app frames it, then the text. `kind`: `search` (the default), the title bar's rounded field, 300pt wide; `filter`, a toolbar's 160pt field; `inline`, a field with no box that fills the rest of its row. `label` names it and shows while it is empty (Search unless given); `value` is the text typed. `focused` rings the search field in the accent; `off` dims a field that has nothing to filter and takes no typing.
  function MacSearchField(props) {
    var kind = props.kind || 'search', label = props.label || 'Search';
    return h('span', { className: cx('bae-m-sfield', 'bae-m-sfield-' + kind, props.focused && 'bae-m-sfield-focused', props.off && 'bae-off') },
      h(Glyph, { set: 'mac', name: 'search', size: kind === 'search' ? 'medium' : 'small', square: true, className: kind === 'filter' ? null : 'bae-m-tertiary' }),
      h('input', { type: 'text', className: 'bae-m-bareinput', defaultValue: props.value || '', placeholder: label, 'aria-label': label, spellCheck: false, disabled: props.off ? true : null }));
  }
  // A file's transfer ring in the Storage Manager's inspector, filling clockwise from the top in the accent over a hairline: `value` from 0 to 1, for a file moving to or from storage or one bae is pinning. `label` names the ring for VoiceOver; without one, the row around it names it.
  function MacProgressRing(props) {
    var C = 2 * Math.PI * 6;
    return h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', className: 'bae-accent', role: props.label ? 'img' : null, 'aria-label': props.label || null, 'aria-hidden': props.label ? null : true },
      h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--hairline)', strokeWidth: 2 }),
      h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeDasharray: (C * Number(props.value)) + ' ' + C, transform: 'rotate(-90 8 8)' }));
  }
  // Found, Imported and Skipped, each a tab named with its count, `tab` the one open (Found unless given).
  function MacImportTabs(props) {
    var tab = props.tab || 'Found';
    var counts = { Found: props.found || '0', Imported: props.imported || '0', Skipped: props.skipped || '0' };
    return h('div', { className: 'bae-m-isb-tabs', role: 'tablist' }, ['Found', 'Imported', 'Skipped'].map(function (t) { var on = t === tab; return h('span', { key: t, role: 'tab', 'aria-selected': on, 'aria-label': t + ', ' + counts[t], className: cx('bae-m-isb-tab', on && 'bae-m-isb-tab-on') }, h('span', null, t), h(StatusChip, { tone: on ? 'accent' : 'neutral' }, counts[t])); }));
  }
  // The identifying ring: how much of the batch is identified (`value`, 0 to 1) in the accent, with round ends, over a strong hairline.
  function identifyingRing(value) {
    var C = 2 * Math.PI * 6;
    return h('svg', { width: 14, height: 14, viewBox: '0 0 16 16', 'aria-hidden': true },
      h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--hairline-strong)', strokeWidth: 2 }),
      h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--accent)', strokeWidth: 2, strokeLinecap: 'round', strokeDasharray: (C * Number(value)) + ' ' + C, transform: 'rotate(-90 8 8)' }));
  }
  // The import list's filter row: the Filter field, the narrowed count, then its buttons: the identifying ring (`identifying`, how far it has got), importing and scanning, each opening what it stands for, and the ⋯ menu with its dot while a filter is on (`filtered`).
  function MacImportFilterRow(props) {
    var ind = function (label, className, child, extra) { return h('button', Object.assign({ type: 'button', className: cx('bae-plainbtn', 'bae-m-isb-ind', className), 'aria-label': label, title: label }, extra), child); };
    return h('div', { className: 'bae-m-isb-filter' },
      h(MacSearchField, { kind: 'inline', label: 'Filter', value: props.filter }),
      props.narrowed ? h('span', { className: 'bae-muted bae-num bae-m-detail' }, props.narrowed) : null,
      // Once text is typed, ⓧ empties the field.
      props.filter ? ind('Clear filter', 'bae-m-tertiary', h(Glyph, { set: 'mac', name: 'removefill', size: 'medium' })) : null,
      props.identifying ? ind('Identifying', null, identifyingRing(props.identifying)) : null,
      props.importing ? ind('Importing…', 'bae-muted', h(Glyph, { set: 'mac', name: 'saveAs', size: 'medium' })) : null,
      // The button's name says what the spinner shows, so the spinner is not read on its own, as the app's button reads.
      props.scanning ? ind('Scanning folders', null, h('span', { 'aria-hidden': true, style: { display: 'flex' } }, h(MacSpinner, { size: 12 }))) : null,
      ind('Sorting, filtering and watched folders', 'bae-muted', [h(Glyph, { key: 'g', set: 'mac', name: 'ellipsis', size: 'medium' }), props.filtered ? h('span', { key: 'd', className: 'bae-m-isb-dot' }) : null], { 'aria-haspopup': 'menu' }));
  }
  // The left column: the tabs, the filter row, then the rows, or `empty`, the line an empty list shows.
  function MacImportSidebar(props) {
    return h('div', { className: 'bae-m-isb', style: props.narrow ? { width: 410 } : null },
      h(MacImportTabs, { tab: props.tab, found: props.found, imported: props.imported, skipped: props.skipped }),
      h('div', { className: 'bae-divider' }),
      h(MacImportFilterRow, { filter: props.filter, narrowed: props.narrowed, identifying: props.identifying, importing: props.importing, scanning: props.scanning, filtered: props.filtered }),
      h('div', { className: 'bae-divider' }),
      props.empty ? h(MacContentUnavailable, { title: props.empty }) : h('div', { className: 'bae-m-isb-list' }, props.children));
  }
  // The release a folder holds, as the import pane shows it beside its cover: the title (`placeholder` when the draft has none yet, in secondary text), the artist, then the release lines: the edition line (`edition`), the copy line (`copy`) and the audio line (`audio`).
  function MacImportReleaseSummary(props) {
    return h('div', { className: 'bae-m-isummary' },
      h('div', { className: cx('bae-m-isummary-title', props.placeholder && 'bae-muted') }, props.placeholder ? 'Album title' : props.title),
      props.artist ? h('div', { className: 'bae-m-isummary-artist' }, props.artist) : null,
      props.edition ? h('div', { className: 'bae-m-isummary-facts' }, props.edition) : null,
      props.copy ? h('div', { className: 'bae-m-isummary-audio' }, props.copy) : null,
      props.audio ? h('div', { className: 'bae-m-isummary-audio' }, props.audio) : null);
  }

  // A way to answer a folder that waits on you, under the releases Identify found: a small button (`title`) over a line saying what it does (`subtitle`).
  function MacAnswerAction(props) {
    return h('div', { className: 'bae-m-answer' }, h(MacButton, { size: 'small', off: props.off }, props.title), h('span', { className: 'bae-m-answer-sub' }, props.subtitle));
  }

  // One catalog a typed search still waits on, under the results: a spinner and "Searching {catalog}…", or, once it failed, the warning status glyph, the catalog's name (`reason` on hover) and Retry. The glyph takes the line's detail text, regular, as the app draws StatusGlyph with no font of its own.
  function MacSearchSourceLine(props) {
    var name = props.catalog || 'Discogs';
    return h('div', { className: 'bae-m-srcline' },
      props.failed
        ? [h(StatusGlyph, { key: 'g', tone: 'warning', set: 'mac', size: tokenPoints('type-detail-size'), weight: 'regular', label: 'Failed' }),
           h('span', { key: 'n', className: 'bae-m-tertiary', title: props.reason || null }, name),
           h('button', { key: 'r', type: 'button', className: 'bae-plainbtn bae-accent' }, 'Retry')]
        : [h(MacSpinner, { key: 's', size: 11 }), h('span', { key: 'n', className: 'bae-m-tertiary' }, 'Searching ' + name + '…')]);
  }

  // The Import section before any folder is added: the sheep, the line that says what to do, Import folder…, and Add Discogs key… while bae has no Discogs key it can use (`discogs` hides it).
  function MacImportEmptyState(props) {
    return h('div', { className: 'bae-m-iempty' },
      h('div', { className: 'bae-m-iempty-art' }, h(ImportSheep, { size: 140 })),
      h('div', { className: 'bae-m-iempty-line' }, 'Add a folder to import music from'),
      h('div', { className: 'bae-m-iempty-actions' }, h(MacButton, null, 'Import folder…'), props.discogs ? null : h(MacButton, null, 'Add Discogs key…')));
  }

  // The Import window's content: the list column (the first child, a MacImportSidebar) beside the pane, which holds the other children. `noSelection`: no folder is picked, and the pane says so in place of the children.
  function MacImportWindow(props) {
    var k = kids(props.children);
    return h('div', { className: 'bae-m-isplit' }, k[0],
      props.noSelection ? h('div', { className: 'bae-m-ipane-empty' }, h(MacContentUnavailable, { title: 'Select a folder', description: 'Choose a scanned folder to search for metadata' }))
        : h('div', { className: 'bae-m-ipane' }, k.slice(1)));
  }
  // A group of folders, a button that folds the group: chevron and the folder's name.
  function MacImportGroupHeader(props) {
    return h('button', { type: 'button', className: 'bae-plainbtn bae-m-igh', 'aria-expanded': !props.closed }, h(Glyph, { set: 'mac', name: 'down', size: 'badge', square: true, className: 'bae-m-tertiary', point: props.closed ? 'forward' : null }), h('span', { className: 'bae-ellipsis' }, props.name));
  }
  // A line of progress in the surrounding font: the phase, a bar filling the rest of the line, and an optional count. No `value` is a wait of no known length.
  function MacProgressLine(props) {
    var value = props.value == null || props.value === '' ? NaN : Number(props.value);
    return h('div', { className: 'bae-m-pline' },
      h('span', { className: 'bae-m-pline-label' }, props.label),
      h(ProgressBar, { value: isNaN(value) ? null : value, className: 'bae-m-pline-bar', label: props.label }),
      props.detail ? h('span', { className: 'bae-m-pline-detail' }, props.detail) : null);
  }
  // One candidate: cover, title over artist (or the folder's name), then while it imports or uploads the step or the upload with its bar, the trailing badge or wait mark. `error` is a skipped folder's reason, under the title and artist or the folder's name.
  function MacImportRow(props) {
    var trail = props.badge ? h(StatusChip, { tone: props.badgeTone || 'warning' }, props.badge)
      : props.state === 'identifyQueued' || props.state === 'importQueued' ? h(Glyph, { set: 'mac', name: 'clock', size: 'small', frame: [12, 12], className: 'bae-muted', label: props.state === 'importQueued' ? 'Queued to import' : 'Queued to identify' })
      : props.state === 'identifying' ? h(MacSpinner, { size: 12 }) : null;
    var line1 = props.folder ? h('div', { className: 'bae-m-irow-folder' }, h(Glyph, { set: 'mac', name: 'folder', size: 'medium', frame: [13, null], className: 'bae-muted' }), h('span', { className: 'bae-mono bae-ellipsis' }, props.folder))
      : h('div', { className: 'bae-m-irow-t' }, h('span', { className: cx('bae-ellipsis', !props.title && 'bae-muted') }, props.title || 'Album title'), props.record ? h(Glyph, { set: 'mac', name: 'external', size: 'medium', frame: [13, 13], className: 'bae-muted', label: 'Identified' }) : h('span', { className: 'bae-m-irow-noarrow' }));
    var line2 = props.folder ? null : h('div', { className: 'bae-m-irow-sub bae-ellipsis' }, props.artist);
    var step = props.state === 'importing' ? h('div', { className: 'bae-m-irow-step' }, h(MacProgressLine, { label: props.step || 'Reading files…', value: props.progress == null ? 0.4 : props.progress }))
      : props.state === 'uploading' ? h('div', { className: 'bae-m-irow-step' }, h(MacProgressLine, { label: 'Uploading', value: props.progress == null ? 0.4 : props.progress }))
      : null;
    var line3 = props.error ? h('div', { className: 'bae-m-irow-sub bae-ellipsis' }, props.error) : null;
    return h('div', { className: cx('bae-m-irow', props.grouped && 'bae-m-irow-grouped', props.selected && 'bae-m-irow-selected') },
      props.grouped ? h('span', { className: 'bae-m-irow-rail' }) : null,
      h(Artwork, { title: props.cover || props.title || props.folder, size: 'size-row-artwork', radius: 'radius-artwork', missing: props.noCover }),
      h('div', { className: 'bae-m-irow-text' }, line1, line2, step, line3), h('span', { style: { flex: 'none', marginLeft: 'auto' } }, trail));
  }
  // Above the right pane: the tab chip, a button that goes to that tab; the folder glyph, a button that reveals the folder in Finder, 15 by 12 as the app's folder symbol is framed; the folder's name.
  function MacImportFolderLine(props) {
    return h('div', { className: 'bae-m-ifl' }, h('button', { type: 'button', className: 'bae-plainbtn bae-m-ifl-tab' }, h(StatusChip, { tone: 'accent' }, props.tab || 'Found')), h(Glyph, { set: 'mac', name: 'down', point: 'forward', size: 'small', className: 'bae-m-tertiary' }), h(Glyph, { set: 'mac', name: 'folder', size: 'medium', className: 'bae-muted', button: 'Reveal in Finder' }), h('span', { className: 'bae-ellipsis bae-strong', style: { fontSize: 'var(--type-heading-size)', lineHeight: 'var(--type-heading-line-height)' } }, props.folder));
  }
  // Where an import goes, one choice of two, as the person last chose it: This Mac or Cloud. One stored choice, shown wherever Import is (the draft card, the bulk pane's Import row) and in Settings → Import, so changing it in one place changes it in all.
  function MacImportDestination(props) { return h(MacPopUpButton, { label: 'Import to', value: props.to, off: props.off }); }
  // A field drawn as the value it holds until the pointer is over it or it has focus, for the draft's values and a track's cells: `label` names it, `placeholder` shows while it is empty. `large` sets it in the album title's size; `quiet` draws its value in the tertiary colour, as the album year.
  // `fit`: as wide as its value, up to its column's edge, and 96pt while empty, for a field in a list row and the album title and year. `off`: dimmed and disabled, as a field in a disabled fieldset is.
  function MacInlineField(props) {
    return h('input', { type: 'text', disabled: props.off ? true : null, className: cx('bae-m-icell', props.large && 'bae-m-icell-large', props.quiet && 'bae-m-icell-quiet', props.fit && 'bae-m-icell-fit'), defaultValue: props.value || '', placeholder: props.placeholder, 'aria-label': props.label, spellCheck: false });
  }
  // A ruled heading over a part of a form: Release, Images, Files, Credits.
  function MacFormHeading(props) { return h('div', { className: 'bae-m-formhead' }, props.children); }
  // The release editor's top row: Identify with Automatic, Search and the Metadata menu (⋯), then the action row: for `actions` `import`, where the import goes (`cloud`, `to`) and Import, or Retry import (`retry`), or neither (`blank`); for `save`, Cancel once something changed (`changed`) and Save.
  function MacIdentifyBar(props) {
    var save = props.actions === 'save';
    return h('div', { className: 'bae-m-idraft-actions' },
      h('span', { className: 'bae-strong bae-muted' }, 'Identify'), h(MacButton, { off: !!props.reading }, 'Automatic'), h(MacButton, { off: !!props.reading }, 'Search'), h(MacButton, { menu: true, label: 'Metadata', off: !!props.reading }, '⋯'),
      h('span', { className: 'bae-m-idraft-trail' },
      props.cloud && !save ? h(MacImportDestination, { to: props.to, off: !!props.reading }) : null,
      // `reading`: the line a save or an import shows while it reads a file on this device before it writes (the audio file a CUE sheet is newly bound to), before the buttons, which are off until the read ends, as the Metadata menu is.
      props.reading ? h('span', { key: 'r', className: 'bae-m-idraft-reading bae-muted' }, h(MacSpinner, { size: 14, label: props.reading })) : null,
      save ? [props.changed ? h(MacButton, { key: 'c', off: !!props.reading }, 'Cancel') : null, h(MacButton, { key: 's', kind: 'primary', off: !!props.reading }, 'Save')] : props.blank ? null : h(MacButton, { kind: 'primary', off: !!props.reading }, props.retry ? 'Retry import' : 'Import')));
  }
  // An artist field, a button that opens the artist picker, drawn as its value until the pointer is over it, like MacInlineField: the artists (`artist`) and their chip (`chip`), New for an artist the import adds or a count, and none when every artist is in the library; `placeholder` in the tertiary colour while it names none (Album artist unless given). `inherited`: a track that takes the album's artists shows Album artist in the tertiary colour. `secondary` draws the artists in secondary text, as the album identity does; `column` fills its column, as a track row's does.
  function MacAlbumArtistField(props) {
    var none = props.inherited || !props.artist;
    return h('button', { type: 'button', className: cx('bae-plainbtn', 'bae-m-iartist', props.column && 'bae-m-iartist-column', props.secondary && 'bae-muted') },
      none ? h('span', { className: 'bae-m-tertiary' }, props.inherited ? 'Album artist' : props.placeholder || 'Album artist')
        : [h('span', { key: 'a', className: 'bae-ellipsis' }, props.artist), props.chip ? h(StatusChip, { key: 'c', tone: props.chip === 'New' ? 'accent' : 'neutral' }, props.chip) : null]);
  }
  // The album the release files under (spec §3.7 step 6, §3.8): Album, then the pop-up showing the album as it stands (`album`): New album, the library album its records name, or one picked. `open` draws its menu: the album as it stands, New album when that album is the library's (`inLibrary`), Use the release’s album when the person picked another (`picked`), a line, then Choose another album…, which searches the library's albums.
  // `off`: the pop-up turned off, while a save reads a file and a pick would cancel it.
  function MacAlbumPicker(props) {
    var items = [props.album].concat(props.inLibrary ? ['New album'] : [], props.picked ? ['Use the release’s album'] : [], ['-', 'Choose another album…']).join(',');
    return h('div', { className: 'bae-m-idraft-album' }, h('span', { className: 'bae-muted' }, 'Album'), h(MacPopUpButton, { value: props.album, label: 'Album', open: props.open, items: items, off: props.off }));
  }
  // The release's cover (a plain tile when it has none) beside its identity: the album title and year as fields and the album artist; then what the card it sits in puts under them, its children: the folder's audio line and the album picker.
  function MacReleaseIdentity(props) {
    return h('div', { className: 'bae-m-idraft-head' },
      props.cover ? h('button', { type: 'button', className: 'bae-plainbtn bae-m-idraft-cover', 'aria-label': 'Change cover', disabled: !!props.coverOff }, h(Artwork, { title: props.cover, size: 132, radius: 'radius-artwork' })) : h(Artwork, { size: 132, radius: 'radius-artwork', missing: true }),
      h('div', { className: 'bae-m-idraft-id' },
        h(MacInlineField, { label: 'Album title', value: props.title, placeholder: 'Album title', large: true, fit: true }),
        h(MacAlbumArtistField, { artist: props.artist, chip: props.artistChip, secondary: true }),
        h(MacInlineField, { label: 'Album year', value: props.year, placeholder: 'Album year', quiet: true, fit: true }),
        kids(props.children)));
  }
  // The folder's audio as the scan read it (spec §3.6: codec, sample rate, bit depth or bitrate, channels), the identity's line under the year.
  function audioLine(audio) { return audio ? h('div', { key: 'audio', className: 'bae-m-tertiary bae-m-idraft-audio' }, audio) : null; }
  // What Discogs says about the pressing that no other field holds: each detail with its remove button, then the menu that adds one more: named Details and showing — while there are none, named Add detail and showing + Add in the accent after.
  // ⊗, which removes one entry of a list, named for the entry (`what`), as xmark.circle.fill in the editor. `off`: dimmed and disabled.
  function removeEntry(what, off) { return h(MacIconButton, { icon: 'removefill', size: 'medium', small: true, faint: true, label: 'Remove ' + what, off: off }); }
  // + and the words, in the accent: the one control every list of the Release grid adds an entry with. `menu`: it opens a menu of what can be added, and says Add. `off`: dimmed and disabled.
  function addEntry(label, menu, off) {
    return h('button', { type: 'button', disabled: off || null, 'aria-haspopup': menu ? 'menu' : null, 'aria-label': label, title: label, className: 'bae-plainbtn bae-accent bae-m-idraft-add' }, h(Glyph, { set: 'mac', name: 'add', size: 'small' }), menu ? 'Add' : label);
  }
  function discogsDetails(details, off) {
    return h('span', { className: 'bae-m-idraft-details' },
      details.map(function (d) { return h('span', { key: d, className: 'bae-m-idraft-detail' }, d, removeEntry(d, off)); }),
      details.length ? addEntry('Add detail', true, off)
        : h('button', { type: 'button', disabled: off || null, className: 'bae-plainbtn bae-m-tertiary', 'aria-haspopup': 'menu', 'aria-label': 'Details' }, '—'));
  }
  // The release's fields under a Release heading: Year, Media, Label with its catalog number, Country, Barcode, Status, Packaging and Details, the typed ones as fields and the chosen ones as pop-ups, each showing — while empty. `off`: every field, stepper and button is disabled, as while a save reads a file.
  function MacReleaseFields(props) {
    // A fact chosen from a menu shows its value as plain text, or the form's dash.
    var off = !!props.off;
    var face = function (label, value) { return h('button', { type: 'button', disabled: off || null, role: 'combobox', 'aria-expanded': false, 'aria-label': label, className: cx('bae-plainbtn', !value && 'bae-m-tertiary') }, value || '—'); };
    var field = function (label, value) { return h(MacInlineField, { label: label, value: value, placeholder: '—', fit: true }); };
    // A list kept one row per entry (labels, barcodes): its name on the first row only, each entry with ⊗, then Add on its own line under the last row; with none, Add alone beside the name.
    var rows = function (name, entries, draw, noun) {
      var addLine = addEntry('Add ' + name.toLowerCase(), false, off);
      if (!entries.length) return [[name, addLine]];
      return entries.map(function (e, i) { return [i === 0 ? name : '', h('span', { className: 'bae-m-idraft-fields' }, draw(e), removeEntry(noun(e), off))]; }).concat([['', addLine]]);
    };
    // `media`: each carrier with how many of it there are, as "2×CD", comma-separated; a carrier alone is one. Each is its name, ×count, a MacStepper and ⊗, on one line, then Add, a menu of the carriers not yet listed.
    var media = list(props.media).map(function (m) { var p = m.split('×'); return p.length > 1 ? { count: p[0], carrier: p[1] } : { count: '1', carrier: p[0] }; });
    var mediaLine = h('span', { className: 'bae-m-idraft-entry bae-m-idraft-media' },
      media.map(function (m) { return h('span', { key: m.carrier, className: 'bae-m-idraft-entry' }, h('span', null, m.carrier), h('span', { className: 'bae-muted bae-num' }, '×' + m.count), h(MacStepper, { label: 'Number of ' + m.carrier, off: off }), removeEntry(m.carrier, off)); }),
      addEntry('Add medium', true, off));
    // `countryOpen`: the countries the Country menu lists, drawn open under its face, with the dash for none first.
    var country = props.countryOpen
      ? h('span', { className: 'bae-m-idraft-pop' }, face('Country', props.country), h('div', { className: 'bae-m-idraft-popmenu' }, h(MacMenu, null, h(MacMenuItem, null, '—'), list(props.countryOpen).map(function (c) { return h(MacMenuItem, { key: c, checked: c === props.country, highlighted: c === props.country }, c); }))))
      : face('Country', props.country);
    var grid = [['Year', field('Year', props.year)], ['Media', mediaLine]]
      // `labels`: each label with the catalog number it gives the pressing, as "Name|Catalog", comma-separated.
      .concat(rows('Label', list(props.labels), function (l) { var p = l.split('|'); return [h(React.Fragment, { key: 'n' }, field('Label', p[0])), h(React.Fragment, { key: 'c' }, field('Catalog number', p[1] || ''))]; }, function (l) { var p = l.split('|'); return p[0] || p[1]; }))
      .concat([['Country', country]])
      // `barcodes`: comma-separated, each a row.
      .concat(rows('Barcode', list(props.barcodes), function (b) { return field('Barcode', b); }, function (b) { return b; }))
      .concat([['Status', face('Status', props.status)], ['Packaging', face('Packaging', props.packaging)], ['Details', discogsDetails(list(props.details), off)]]);
    var cells = h('div', { className: 'bae-m-idraft-grid' }, grid.map(function (r, i) { return [h('span', { key: 'l' + i, className: 'bae-muted' }, r[0]), h('span', { key: 'v' + i, className: 'bae-m-idraft-cell' }, r[1])]; }));
    return h('div', { className: 'bae-m-relfields' },
      h(MacFormHeading, null, 'Release'),
      // A disabled fieldset disables every field and button in the grid, which dims the fields and the plain buttons; each ⊗ and stepper takes `off`, which dims it.
      off ? h('fieldset', { disabled: true, 'aria-disabled': true, style: { display: 'contents' } }, cells) : cells);
  }
  // The release's identity and its fields, as both cards draw them under their lead: the cover and identity with the audio line and the album picker, then the Release grid.
  function releaseHeader(props) {
    return [
      h(MacReleaseIdentity, { key: 'id', cover: props.cover, coverOff: !!props.reading, title: props.title, artist: props.artist, artistChip: props.artistChip, year: props.year },
        audioLine(props.audio),
        props.album ? h(MacAlbumPicker, { key: 'album', album: props.album, inLibrary: props.albumInLibrary, picked: props.albumPicked, open: props.albumOpen, off: !!props.reading }) : null),
      h(MacReleaseFields, { key: 'fields', year: props.pressingYear, media: props.media, labels: props.labels, country: props.country, countryOpen: props.countryOpen, barcodes: props.barcodes, status: props.status, packaging: props.packaging, details: props.details, off: !!props.reading })];
  }
  // The release editor, one component in the import pane and in the album's Edit release sheet; its `actions` row is the only difference. `actions` `import`: where the import goes (`cloud`, `to`) and Import, or Retry import (`retry`), or neither while the folder has nothing to import (`blank`), with the folder's `check` and the audio a picked release leaves out (`leftOut`) in warning notices under that row. `actions` `save`: Cancel once something changed (`changed`), and Save. `reading`: the file a save or an import reads before it writes, while the cover, Unlink, the Metadata menu and the album pop-up are off, since a click on any of them would cancel the save, and the identity, the fields, the track table and the Files list are off too. Everything off is dimmed but the cover. Then, in both: the release's identity and fields; the records under a rule, with Unlink; the folder's or release's images (`images`); the track table, the children (side headings, CUE captions and track rows); the Files list (`files`). `searching`: the first child, identification's results (MacFindOnline), stands in place of the card until a pick or Back, and the images, the track table and the Files list stay under it.
  function MacReleaseEditor(props) {
    var k = kids(props.children), leftOut = list(props.leftOut);
    var tracks = props.searching ? k.slice(1) : k;
    function editable(children) {
      return props.reading ? h('fieldset', { disabled: true, 'aria-disabled': true, style: { display: 'contents' } }, children) : children;
    }
    return h('div', { className: 'bae-m-reditor' },
      props.searching ? k[0] : h('div', { className: 'bae-m-card bae-m-idraft' },
        h(MacIdentifyBar, { actions: props.actions, blank: props.blank, retry: props.retry, cloud: props.cloud, to: props.to, changed: props.changed, reading: props.reading }),
        props.check && props.actions !== 'save' ? h(MacNotice, { tone: 'warning', title: props.check }) : null,
        // `leftOut`: the folder's audio files that no track of the picked release matches, comma-separated, which the import leaves out. A warning notice says so, names each file, and offers Pick another release…, which opens Search.
        leftOut.length > 0 && props.actions !== 'save' ? h(MacNotice, { tone: 'warning', title: 'Audio left out', subtitle: 'No track on this release matches ' + (leftOut.length === 1 ? 'this file, so it stays' : 'these files, so they stay') + ' out of your library. To import ' + (leftOut.length === 1 ? 'it' : 'them') + ', pick another release.', action: 'Pick another release…' }, leftOut.map(function (f) { return h('div', { key: f, className: 'bae-mono bae-ellipsis' }, f); })) : null,
        editable(releaseHeader(props)),
        props.records ? h('div', { className: 'bae-m-idraft-records' }, h(MacRecordLinks, { records: props.records }), h('span', { style: { flex: 1 } }), h('button', { type: 'button', className: 'bae-plainbtn bae-accent', disabled: !!props.reading }, 'Unlink')) : null),
      props.images ? h(MacImportImages, { images: props.images }) : null,
      tracks.length ? editable(h(MacImportTable, null, tracks)) : null,
      props.files ? editable(h(MacReleaseFiles, { files: props.files })) : null);
  }
  // A skipped folder's card, read-only: why bae can't import it (`reason`) in a warning notice where Import would be, then its cover and what its tags call the release (`title`, `artist`), or its `folder` name when they call it nothing.
  function MacInvalidCandidateCard(props) {
    return h('div', { className: 'bae-m-card bae-m-idraft' },
      h(MacNotice, { tone: 'warning', title: props.reason }),
      h('div', { className: 'bae-m-idraft-head' },
        h(Artwork, { title: props.cover, size: 132, radius: 'radius-artwork', missing: !props.cover }),
        props.title ? h(MacImportReleaseSummary, { title: props.title, artist: props.artist }) : h('span', { className: 'bae-mono' }, props.folder)));
  }
  // The folder's images under the release: a ruled Images heading, then each image at 128pt, a button named by its file that opens the lightbox at it (`images`: "artwork title|file name", comma-separated).
  function MacImportImages(props) {
    return h('div', { className: 'bae-m-ifiles' }, h(MacFormHeading, null, 'Images'),
      h('div', { className: 'bae-m-iimages' }, list(props.images).map(function (t) { var p = t.split('|'); return h('button', { key: t, type: 'button', className: 'bae-plainbtn bae-m-iimage', 'aria-label': p[1] }, h(Artwork, { title: p[0], size: 128, radius: 'radius-artwork' })); })));
  }
  // One file of a release: its name in mono and its size at the right. `kind`: `opens` (the default), a document or a track sheet, a button named by its file that opens it; `audio`, a track that plays, with its Play button before the size; `other`, a file that opens nothing. `damaged`: why the file is damaged, marked in the warning colour; a damaged file opens nothing.
  function MacReleaseFileRow(props) {
    var kind = props.kind || 'opens', opens = kind === 'opens' && !props.damaged;
    return h(opens ? 'button' : 'div', { type: opens ? 'button' : null, className: cx(opens && 'bae-plainbtn', 'bae-m-ifile'), 'aria-label': opens ? props.name : null },
      h('span', { className: 'bae-mono bae-ellipsis' }, props.name),
      props.damaged ? h('span', { className: 'bae-m-ifile-damaged' }, h(StatusGlyph, { tone: 'warning', set: 'mac', size: 'small', weight: 'semibold' }), props.damaged) : null,
      kind === 'audio' ? h(Glyph, { set: 'mac', name: 'play', size: 'chip', weight: 'bold', frame: [10, null], contentShape: true, className: 'bae-muted bae-m-ifile-play', button: 'Play' }) : null,
      h('span', { className: 'bae-m-tertiary bae-m-ifile-size' }, props.size || ''));
  }
  // The release's files that are no track (a rip log, a track sheet that carves no tracks; never an image bae can show), or on a skipped folder's pane every file but its images: a ruled Files heading, then a row per file (`files`: "name|size[|kind]", comma-separated, where kind is `other`, `audio` or `damaged:why`, and a file with none opens).
  // A kind of `ignored:audio` is a CUE sheet the person set to Ignored, which carves no tracks, so it sits in its place in the list as its caption, its place pop-up reading Ignored and its FILE line playing from `audio`, and the person can place it back.
  function MacReleaseFiles(props) {
    return h('div', { className: 'bae-m-ifiles' }, h(MacFormHeading, null, 'Files'),
      h('div', null, list(props.files).map(function (f) {
        var p = f.split('|'), kind = p[2] || '', damaged = kind.indexOf('damaged:') === 0;
        if (kind.indexOf('ignored:') === 0) return h(MacImportSheetCaption, { key: p[0], disc: 'Ignored', sheet: p[0], bound: kind.slice('ignored:'.length) });
        return h(MacReleaseFileRow, { key: p[0], name: p[0], size: p[1], kind: damaged ? null : kind || null, damaged: damaged ? kind.slice('damaged:'.length) : null });
      })));
  }
  // The mapping table: Source | Track | Title | Artist | Length, a side's eyebrow, CUE captions and rows.
  // The table spaces each side's eyebrow, as the app's table does: 2pt above the first side and 16pt above the others, 6pt under each.
  function MacImportTable(props) {
    var side = 0;
    var rows = kids(props.children).map(function (c, i) {
      var p = inner(c);
      return p && p.type === MacEyebrow ? h('div', { key: 'side' + i, className: cx('bae-m-itable-side', side++ === 0 && 'bae-m-itable-side-first') }, c) : c;
    });
    return h('div', { className: 'bae-m-itable-scroll' }, h('div', { className: 'bae-m-itable' }, h('div', { className: 'bae-m-itable-row bae-m-itable-head' }, ['Source', 'Track', 'Title', 'Artist', 'Length'].map(function (t) { return h('span', { key: t, className: 'bae-m-eyebrowtxt' }, t); })), rows));
  }
  // `start`: a CUE image's track, its source cell where the track starts in the image (the sheet's caption names the image once). `listed`: the length the source lists, after the measured `length` in the warning colour, when the two are over 3 s apart, so the row says what disagrees. The title and the artist are fields; an artist the track inherits from the album leaves its field empty, showing Album artist.
  // `added`: a track a save adds and has not made yet, which plays nothing, so it has no Play button. Its column stays, empty, so the source keeps the other rows' edge.
  function MacImportTrackRow(props) {
    return h('div', { className: cx('bae-m-itable-row', 'bae-m-itable-track', props.playing && 'bae-m-itable-playing') },
      h('span', { className: 'bae-m-isrc' }, props.added ? h('span', { className: 'bae-m-isrc-noplay', 'aria-hidden': true }) : h(Glyph, { set: 'mac', name: props.playing ? 'stop' : 'play', size: 'chip', weight: 'bold', frame: [10, 16], contentShape: true, className: props.playing ? 'bae-accent' : 'bae-muted', button: props.playing ? 'Stop' : 'Play' }), props.start != null ? h('span', { className: 'bae-num bae-muted' }, props.start) : h('span', { className: 'bae-mono bae-ellipsis' }, props.file)),
      h('span', { className: 'bae-num' }, props.position),
      h('span', { className: 'bae-m-itable-cell' }, h(MacInlineField, { label: 'Title', value: props.title, placeholder: 'Untitled' })),
      h('span', { className: 'bae-m-itable-cell' }, h(MacAlbumArtistField, { artist: props.artist, chip: props.artistChip, inherited: props.inherited, placeholder: 'Artist', column: true })),
      h('span', { className: 'bae-num', style: { textAlign: 'right' } }, props.length, props.listed ? h('span', { className: 'bae-warning', title: 'Listed length' }, ' ' + props.listed) : null));
  }
  // A CUE sheet's caption: the pop-up of which side or disc the sheet is on (Side A on a record or cassette, Disc 1 elsewhere), CUE, the sheet's name (a button that opens it in the viewer), → and the pop-up of the audio its FILE line is bound to.
  function MacImportSheetCaption(props) {
    return h('div', { className: 'bae-m-icue' },
      h('button', { type: 'button', role: 'combobox', 'aria-label': 'Side or disc', 'aria-expanded': false, className: 'bae-plainbtn bae-m-icue-disc' }, h('span', { className: 'bae-strong' }, props.disc), h(Glyph, { set: 'mac', name: 'down', size: 'badge' })),
      h(StatusChip, { tone: 'accent' }, 'CUE'),
      h('button', { type: 'button', className: 'bae-plainbtn bae-mono' }, props.sheet),
      h('span', { className: 'bae-m-tertiary', 'aria-hidden': true }, '→'),
      h('button', { type: 'button', role: 'combobox', 'aria-label': 'Audio file', 'aria-expanded': false, className: cx('bae-plainbtn', 'bae-mono', props.unbound ? 'bae-warning' : 'bae-muted') }, props.bound || 'Choose audio…'));
  }
  // Find online: Back (a button) and Identify, the Discogs bar, then Automatic and Search, each heading a button that opens its section, one open at a time; the open section's heading is the current one, as the Mac marks it selected. `automatic`: the Automatic run's state, `not-started`, `working`, `failed`, `nothing-found` or `found` (the default); `chips`, `titleSearch` and `offered`: its identifier band (MacIdentifierBand). `sentence`: why the folder waits on the person, heading Automatic over a rule. The children fill Search when it is open, and follow the band in Automatic, or follow Search manually when nothing was found.
  function MacFindOnline(props) {
    var open = props.open || 'Automatic';
    var head = function (t, status) { return h('button', { type: 'button', 'aria-label': t, 'aria-expanded': open === t, 'aria-current': open === t ? 'true' : null, className: cx('bae-plainbtn', 'bae-m-fo-sec', open !== t && props.dimmed === t && 'bae-m-fo-dim') }, h(Glyph, { set: 'mac', name: 'down', size: 'badge', square: true, className: 'bae-m-tertiary', point: open === t ? null : 'forward' }), h('span', { className: 'bae-m-eyebrowtxt' }, t), status ? h('span', { className: 'bae-m-fo-status' }, status) : null); };
    var k = kids(props.children);
    var auto = props.automatic || 'found';
    return h('div', { className: 'bae-m-card bae-m-fo' },
      h('div', { className: 'bae-m-fo-head' }, h('button', { type: 'button', className: 'bae-plainbtn bae-accent bae-m-fo-back' }, h(Glyph, { set: 'mac', name: 'down', point: 'backward', size: 'small' }), 'Back'), h('span', { className: 'bae-strong', style: { position: 'absolute', left: 0, right: 0, textAlign: 'center', pointerEvents: 'none' } }, 'Identify')),
      h('div', { className: 'bae-divider' }),
      // The Discogs bar is the shared notice: its title, what to do, Open settings and the × that dismisses it.
      props.discogsBar ? h('div', { style: { padding: 'var(--space-group)' } }, h(MacNotice, { tone: 'info', title: 'Add Discogs to find more pressings', subtitle: 'Many regional releases, reissues and vinyl pressings are only on Discogs. Tokens are free: create one in your Discogs account settings.', action: 'Open settings', dismiss: true })) : null,
      // Automatic's header ends in what its run has to say: the spinner while it works, the warning glyph when it failed, 0 when nothing matched.
      head('Automatic', auto === 'working' ? h(MacSpinner, { size: 11 }) : auto === 'failed' ? h(StatusGlyph, { tone: 'warning', set: 'mac', size: 'small', weight: 'semibold', label: 'Failed' }) : auto === 'nothing-found' ? h(StatusChip, null, '0') : null),
      open === 'Automatic' ? h('div', { className: 'bae-m-fo-body' }, automaticContent(auto, props.sentence, props.chips ? { chips: props.chips, titleSearch: props.titleSearch, offered: props.offered } : null, k)) : null,
      h('div', { className: 'bae-divider' }),
      head('Search', null),
      open === 'Search' ? h('div', { className: 'bae-m-fo-body' }, k) : null);
  }
  // Automatic's section from its run's state: before any run, Search manually alone; otherwise the sentence over a rule, the run's identifier band (`band`, the band's props) over a rule, then No results, Search manually and the children (the way to answer) when nothing matched, or else the children (what the run found).
  function automaticContent(auto, sentence, band, found) {
    var searchManually = function (key) { return h(MacButton, { key: key }, 'Search manually'); };
    if (auto === 'not-started') return h('div', { className: 'bae-m-fo-empty bae-m-fo-empty-alone' }, searchManually('s'));
    return [
      sentence ? h('div', { key: 's' }, sentence) : null,
      sentence ? h('div', { key: 'r', className: 'bae-divider' }) : null,
      band ? h(MacIdentifierBand, Object.assign({ key: 'b' }, band)) : null,
      band ? h('div', { key: 'd', className: 'bae-divider' }) : null,
      auto === 'nothing-found' ? h('div', { key: 'n', className: 'bae-m-fo-empty bae-m-fo-empty-below' }, h('span', { className: 'bae-muted' }, 'No results'), searchManually('s'), found) : null,
      auto === 'nothing-found' ? null : h(React.Fragment, { key: 'f' }, found)];
  }
  // One identifier the folder's files state, in one of three looks (`state`): `off`, an outline, for an identifier the run leaves out, struck out or not yet put in, such as a catalog number the folder offers that no release confirms; `on` (the default), the hover fill, for one the run asks about; `found`, the accent's tint, for one that found releases. `dash`: a dash after the label for an identifier the files leave out, with no fill and no ring.
  // The label is in the chip type, the value on a dark inset of the chip's fill in the detail type. A chip of several values (" · "-separated, the files' ISRCs) shows the first, then a +N button that opens a popover listing every one, a line each; `moreShown` draws that popover open, after the chip. The band sets `moreOpen` instead, which shows the button open and leaves the popover to the band.
  // `toggles`: the chip is a button that leaves its identifier out of the run or puts it back. `fields`: the title search's words, "Artist=…,Title=…", each name followed by its field in place of the label and value.
  function MacIdentifierChip(props) {
    var st = props.state || 'on';
    var values = String(props.value || '').split(' · '), several = values.length > 1;
    // A chip that toggles is one button, which VoiceOver reads as its parts with a comma between: "Barcode, 0 41234 56789 2".
    var name = props.toggles ? [props.label, props.value].filter(Boolean).join(', ') : null;
    var moreOpen = !!(props.moreShown || props.moreOpen);
    var label = function (key, text) { return h('span', { key: key, className: 'bae-m-idchip-name' }, text); };
    var fields = props.fields ? list(props.fields).map(function (f) { var q = f.split('='); return [label('n' + q[0], q[0]), h('input', { key: 'f' + q[0], type: 'text', className: 'bae-m-idchip-value bae-m-idchip-field', defaultValue: q[1] || '', 'aria-label': q[0], spellCheck: false })]; }) : null;
    var content = fields || [label('l', props.label),
      st === 'dash' ? h('span', { key: 'd', className: 'bae-m-idchip-dashmark' }) : h('span', { key: 'v', className: 'bae-m-idchip-value' }, values[0]),
      several ? h('span', { key: 'm', className: cx('bae-m-idchip-more', moreOpen && 'bae-m-idchip-more-open') }, h('button', { type: 'button', className: 'bae-plainbtn', 'aria-expanded': moreOpen }, '+' + (values.length - 1))) : null];
    var chip = h(props.toggles ? 'button' : 'span', { type: props.toggles ? 'button' : null, 'aria-label': name, className: cx(props.toggles && 'bae-plainbtn', 'bae-m-idchip', 'bae-m-idchip-' + st) }, content);
    return props.moreShown && several ? h('span', { className: 'bae-m-idchip-shown' }, chip, valuesPopover(values)) : chip;
  }
  // The popover of every value of a chip, a line each in the value's type, hanging from the chip's open +N button. It is a window of its own, so it comes after everything in the window the button is in.
  function valuesPopover(values) {
    return h('span', { className: 'bae-m-idchip-pop' }, h(MacPopover, { width: 150, top: 10, right: -75, arrow: 75 }, h('div', { className: 'bae-m-idchip-codes' }, values.map(function (v, i) { return h('div', { key: i }, v); }))));
  }
  // The identifier chips: "Label|value|state", comma-separated, each state `off`, `on`, `found` or `dash` (MacIdentifierChip). A Disc ID, Barcode or Catalog # chip that holds a value toggles. `titleSearch`: the words the run searches by, "artist|title|state", one chip after the others that reads Artist and Title, each followed by its field, `found` unless given.
  // `offered`: the catalog numbers the folder offers that no release confirms, comma-separated, each an off Catalog # chip that puts its number into the run. The band folds them after the other chips, behind a "+N more" button with their count. `candidates`: `folded` (the default) or `shown`, which draws every one, then the button, which reads Show fewer. `open`: the label of the chip whose +N popover is drawn open, such as ISRC, after the band.
  var TOGGLED_IDENTIFIERS = ['Disc ID', 'Barcode', 'Catalog #'];
  function MacIdentifierBand(props) {
    var parts = list(props.chips).map(function (c) { return c.split('|'); });
    var chip = function (p, i) { return h(MacIdentifierChip, { key: i, label: p[0], value: p[1], state: p[2], toggles: TOGGLED_IDENTIFIERS.indexOf(p[0]) >= 0 && !!p[1] && p[2] !== 'dash', moreOpen: p[0] === props.open }); };
    var opened = parts.filter(function (p) { return p[0] === props.open; })[0];
    var title = props.titleSearch ? String(props.titleSearch).split('|') : null;
    var offered = list(props.offered), shown = props.candidates === 'shown';
    return h('div', { className: 'bae-m-band' },
      parts.map(chip),
      title ? h(MacIdentifierChip, { key: 't', fields: 'Artist=' + title[0] + ',Title=' + (title[1] || ''), state: title[2] || 'found' }) : null,
      shown ? offered.map(function (n, i) { return chip(['Catalog #', n, 'off'], 'c' + i); }) : null,
      offered.length ? h('button', { type: 'button', className: 'bae-plainbtn', title: 'Catalog numbers found in the folder that no release confirms' }, h(StatusChip, null, shown ? 'Show fewer' : '+' + offered.length + ' more')) : null,
      opened ? valuesPopover(opened[1].split(' · ')) : null);
  }
  // Every catalog that describes a release, as a wrapped row of links to each catalog's page, in the order core lists them (`records`: catalog names, comma-separated).
  // `buttons`: each is a button that opens the page, as on a found album's card, rather than a link.
  // Each link ends in arrow.up.right at the chip text's small image scale, which draws it at 8.3 points, in the 12 point square the app frames it in.
  function MacRecordLinks(props) {
    return h('span', { className: 'bae-m-reclinks' }, list(props.records).map(function (s) { return h(props.buttons ? 'button' : 'a', props.buttons ? { key: s, type: 'button', className: 'bae-m-reclink bae-m-reclink-button' } : { key: s, href: '#', className: 'bae-m-reclink' }, s, h(Glyph, { set: 'mac', name: 'external', size: 8.3, frame: [12, 12] })); }));
  }
  // A found album: cover, title, artist · label, a button per catalog that has answered (`sources`) that opens the album on that catalog.
  function MacReleaseCard(props) {
    return h('div', { className: 'bae-m-rcard-head' }, h(Artwork, { title: props.title, size: 'size-row-artwork', radius: 'radius-artwork' }), h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-strong', style: { fontSize: 'var(--type-heading-size)', lineHeight: 'var(--type-heading-line-height)' } }, props.title), h('div', { className: 'bae-muted' }, props.artist + (props.label ? ' · ' + props.label : ''))), props.sources ? h(MacRecordLinks, { records: props.sources, buttons: true }) : null);
  }
  // A found album with its pressings: the MacReleaseCard (`title`, `artist`, `label`, `sources`), then its pressings, the children, on a rule under it.
  function MacReleaseGroup(props) {
    return h('div', { className: 'bae-m-rcard' },
      h(MacReleaseCard, { title: props.title, artist: props.artist, label: props.label, sources: props.sources }),
      h('div', { className: 'bae-m-rcard-rows' }, props.children));
  }
  // A fact about a pressing. A fact the pressing's records state differently is every record's value, each followed by its catalog's name: "1992 MusicBrainz / 1993 Discogs", written `value` "1992:MusicBrainz|1993:Discogs". A fact one record states alone is shown as stated. It draws in the type of the text around it.
  function MacSourcedValue(props) {
    var text = props.value;
    if (String(text).indexOf('|') < 0) return text;
    return sourcedParts(text).map(function (p, i) {
      return h(React.Fragment, { key: i }, i ? h('span', { className: 'bae-m-tertiary' }, ' / ') : null, p.value, h('span', { className: 'bae-m-tertiary' }, ' ' + p.catalog));
    });
  }
  // The values of a sourced fact with the catalog each names, from "1992:MusicBrainz|1993:Discogs".
  function sourcedParts(text) {
    return String(text).split('|').map(function (v) { var at = v.lastIndexOf(':'); return { value: v.slice(0, at), catalog: v.slice(at + 1) }; });
  }
  // A found pressing: its edition line (`edition`: " · "-separated facts, the year, country, each label and its catalog number), then its copy line (`copy`: " · "-separated facts, the media and details) with the barcode and badges after it, then `notes` (the records' notes, a release's disambiguation, with the Notes badge), each fact a MacSourcedValue; In library, dimmed whole, for one the library has; `sources`, the catalogs whose records it holds, only those that have answered; the chevron.
  // In library's check is the success status glyph in the detail text's font, regular.
  function MacPressingRow(props) {
    var sv = function (v) { return h(MacSourcedValue, { value: v }); };
    var line = function (text) { return text ? String(text).split(' · ').map(function (part, i) { return h(React.Fragment, { key: i }, i ? ' · ' : null, sv(part)); }) : null; };
    var edition = line(props.edition), copy = line(props.copy);
    return h('div', { className: cx('bae-m-prow', props.selected && 'bae-m-prow-selected'), style: props.inLibrary ? { opacity: 0.55 } : null },
      h('div', { style: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 'var(--space-line)' } },
        edition ? h('div', { className: 'bae-num', style: { whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' } }, edition) : null,
        h('div', { style: { display: 'flex', gap: 'var(--space-compact)', alignItems: 'center', flexWrap: 'wrap' } }, copy ? h('span', { className: 'bae-m-tertiary bae-m-detail' }, copy) : null, props.barcode ? h('span', { className: 'bae-muted bae-num' }, sv(props.barcode)) : null, list(props.badges).map(function (b) { return h(StatusChip, { key: b, tone: 'accent' }, b); }), props.notes ? h('span', { title: String(props.notes).indexOf('|') < 0 ? props.notes : sourcedParts(props.notes).map(function (p) { return p.value + ' ' + p.catalog; }).join(' / ') }, h(StatusChip, { tone: 'accent' }, 'Notes')) : null),
        // `notes`: the records' notes (a release's disambiguation), every record's that states one, each with its catalog when they differ. The Notes badge and this line come from the same records, so they show together.
        props.notes ? h('div', { className: 'bae-m-tertiary bae-m-detail', style: { fontStyle: 'italic' } }, sv(props.notes)) : null),
      props.inLibrary ? h('span', { style: { display: 'flex', alignItems: 'center', gap: 'var(--space-inline)' }, className: 'bae-m-tertiary bae-m-detail' }, h(StatusGlyph, { tone: 'success', set: 'mac', size: tokenPoints('type-detail-size'), weight: 'regular' }), h('span', { className: 'bae-m-prow-trail' }, 'In library')) : null,
      // `sources`: the catalogs whose records the pressing holds, only those that have answered.
      props.sources ? h('span', { className: 'bae-m-tertiary bae-m-detail bae-m-prow-trail' }, props.sources) : null,
      h(Glyph, { set: 'mac', name: 'down', point: 'forward', size: 'small', frame: [11, null], className: props.selected ? 'bae-accent' : 'bae-m-tertiary' }));
  }
  // One bulk action: its name, then how many of the selected folders it applies to (`count`), dimmed when none. `strong` sets the name in the strong type and the count in the accent, for Import and Combine. `to`: where the import goes, Import's destination pop-up, before the count; the row's button then holds only the name, so the pop-up is a control of its own.
  function MacBulkActionRow(props) {
    var n = props.count, off = !n, row = cx('bae-m-bulkrow', off && 'bae-m-bulkrow-off'), type = props.strong ? 'bae-strong' : 'bae-muted';
    var chip = h(StatusChip, { tone: props.strong ? 'accent' : 'neutral' }, String(n || 0));
    if (props.to == null) return h('button', { type: 'button', disabled: off || null, className: row }, h('span', { className: type, style: { flex: 1 } }, props.label), chip);
    return h('div', { className: row },
      h('button', { type: 'button', disabled: off || null, className: 'bae-plainbtn ' + type, 'aria-label': props.label + ' ' + props.to + ' ' + (n || 0) }, props.label),
      h('span', { style: { flex: 1 } }), h(MacImportDestination, { to: props.to, off: off }), chip);
  }
  // Several rows selected: the count, then the actions in groups, each with how many it applies to.
  function MacImportBulk(props) {
    // `to` puts Import's destination on Import's own line, before its count.
    var row = function (label, n, strong, to) { return h(MacBulkActionRow, { key: label, label: label, count: n, strong: strong, to: to }); };
    var grp = function (eyebrow, rows) { return h('div', { className: 'bae-m-bulkgrp' }, eyebrow ? h('div', { className: 'bae-m-eyebrowtxt', style: { padding: 'var(--space-related) var(--space-group) var(--space-line)' } }, eyebrow) : null, rows); };
    var n = Number(props.count || 5);
    return h('div', { className: 'bae-m-bulk' }, h('div', { className: 'bae-m-card', style: { width: 440, boxSizing: 'border-box', padding: 'var(--space-section)', display: 'flex', flexDirection: 'column', gap: 'var(--space-group)' } },
      h('div', { style: { font: 'var(--type-title-weight) var(--type-title-size) var(--font-sans)' } }, n + ' selected'), h('div', { className: 'bae-muted' }, 'Each action applies only to eligible selected folders.'),
      h('div', { className: 'bae-m-bulklist' },
        grp('Import', [row('Import', props.ready || 3, true, props.cloud ? props.to : null), row('Cancel import of selected', 0), row('Combine as one release', 0, true), row('Keep as separate releases', 0)]),
        grp('Metadata', [row('Identify selected', n), row('Stop identifying selected', 0), row('Retry failed identification', 1), row('Reset to file metadata', n), row('Clear metadata', n)]),
        grp(null, [row('Skip selected', n), row('Restore to pending', 0)]),
        grp(null, [row('Reveal in Finder', n)]))),
      props.running ? h('div', { style: { width: 440, display: 'flex', gap: 'var(--space-group)', alignItems: 'flex-end' } }, h('div', { style: { flex: 1 } }, h(MacProgress, { label: props.running, value: 0.4 })), h(MacButton, null, 'Cancel')) : null);
  }
