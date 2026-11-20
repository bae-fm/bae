  // ==== part: import ====
  SYM.stop = 'M7 7h10v10H7z';
  // ---- Import (macOS) ----
  // A search or filter field: the magnifier, then the text. `kind`: `search` (the default), the title bar's rounded field, 300pt wide; `filter`, a toolbar's 160pt field; `inline`, a field with no box that fills the rest of its row. `label` names it and shows while it is empty (Search unless given); `value` is the text typed. `focused` rings the search field in the accent; `off` dims a field that has nothing to filter and takes no typing.
  function MacSearchField(props) {
    var kind = props.kind || 'search', label = props.label || 'Search';
    return h('span', { className: cx('bae-m-sfield', 'bae-m-sfield-' + kind, props.focused && 'bae-m-sfield-focused', props.off && 'bae-off') },
      h(Glyph, { set: 'mac', name: 'search', size: kind === 'search' ? 14 : 12, className: kind === 'inline' ? 'bae-m-tertiary' : null }),
      h('input', { type: 'text', className: 'bae-m-bareinput', defaultValue: props.value || '', placeholder: label, 'aria-label': label, spellCheck: false, disabled: props.off ? true : null }));
  }
  // A file's transfer ring in the Storage Manager's inspector, filling clockwise from the top: `value` from 0 to 1. `tone`: `activity` (the default), a file moving to or from storage, or `pinning`, a file bae is pinning, over a hairline. `label` names the ring for VoiceOver; without one, the row around it names it.
  function MacProgressRing(props) {
    var C = 2 * Math.PI * 6, tone = props.tone || 'activity';
    return h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', className: 'bae-' + tone, role: props.label ? 'img' : null, 'aria-label': props.label || null, 'aria-hidden': props.label ? null : true },
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
      props.narrowed ? h('span', { className: 'bae-muted bae-num', style: { fontSize: 11.5 } }, props.narrowed) : null,
      props.identifying ? ind('Identifying', null, identifyingRing(props.identifying)) : null,
      props.importing ? ind('Importing…', 'bae-muted', h(Sym, { name: 'saveAs', size: 14 })) : null,
      props.scanning ? ind('Scanning folders', null, h(MacSpinner, { size: 12 })) : null,
      ind('Sorting, filtering and watched folders', 'bae-muted', [h(Glyph, { key: 'g', set: 'mac', name: 'ellipsis', size: 15 }), props.filtered ? h('span', { key: 'd', className: 'bae-m-isb-dot' }) : null], { 'aria-haspopup': 'menu' }));
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
  // The release a folder holds, as the import pane shows it beside its cover: the title (`placeholder` when the draft has none yet, in secondary text), the artist, the facts line (`facts`) and the audio the scan read (`audio`).
  function MacImportReleaseSummary(props) {
    return h('div', { className: 'bae-m-isummary' },
      h('div', { className: cx('bae-m-isummary-title', props.placeholder && 'bae-muted') }, props.placeholder ? 'Album title' : props.title),
      props.artist ? h('div', { className: 'bae-m-isummary-artist' }, props.artist) : null,
      props.facts ? h('div', { className: 'bae-m-isummary-facts' }, props.facts) : null,
      props.audio ? h('div', { className: 'bae-m-isummary-audio' }, props.audio) : null);
  }

  // A way to answer a folder that waits on you, under the releases Identify found: a small button (`title`) over a line saying what it does (`subtitle`).
  function MacAnswerAction(props) {
    return h('div', { className: 'bae-m-answer' }, h(MacButton, { size: 'small', off: props.off }, props.title), h('span', { className: 'bae-m-answer-sub' }, props.subtitle));
  }

  // One catalog a typed search still waits on, under the results: a spinner and "Searching {catalog}…", or, once it failed, a warning mark, the catalog's name (`reason` on hover) and Retry.
  function MacSearchSourceLine(props) {
    var name = props.catalog || 'Discogs';
    return h('div', { className: 'bae-m-srcline' },
      props.failed
        ? [h('svg', { key: 'g', width: 11, height: 11, viewBox: '0 0 24 24', role: 'img', 'aria-label': 'Failed', className: 'bae-m-srcline-failed', fill: 'none', stroke: 'currentColor', strokeWidth: 2.2, strokeLinecap: 'round' }, h('circle', { cx: 12, cy: 12, r: 9.5 }), h('path', { d: 'M12 7v6M12 16.6v.4' })),
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
      : props.state === 'identifyQueued' || props.state === 'importQueued' ? h('span', { className: 'bae-muted', role: 'img', 'aria-label': props.state === 'importQueued' ? 'Queued to import' : 'Queued to identify', title: props.state === 'importQueued' ? 'Queued to import' : 'Queued to identify' }, h(Sym, { name: 'clock', size: 12 }))
      : props.state === 'identifying' ? h(MacSpinner, { size: 12 }) : null;
    var line1 = props.folder ? h('div', { className: 'bae-m-irow-folder' }, h(Sym, { name: 'folder', size: 13, className: 'bae-muted' }), h('span', { className: 'bae-mono bae-ellipsis', style: { fontSize: 12 } }, props.folder))
      : h('div', { className: 'bae-m-irow-t' }, h('span', { className: cx('bae-ellipsis', !props.title && 'bae-muted') }, props.title || 'Album title'), h('span', props.record ? { className: 'bae-linkout', role: 'img', 'aria-label': 'Identified' } : { className: 'bae-linkout', style: { opacity: 0 }, 'aria-hidden': true }, h(Sym, { name: 'external', size: 13, className: 'bae-muted' })));
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
  function MacImportDestination(props) { return h(MacPopUpButton, { label: 'Import to', value: props.to, off: props.off }); }
  // A field drawn as the value it holds until the pointer is over it or it has focus, for the draft's values and a track's cells: `label` names it, `placeholder` shows while it is empty. `large` sets it in the album title's size; `quiet` draws its value in the tertiary colour, as the album year.
  function MacInlineField(props) {
    return h('input', { type: 'text', className: cx('bae-m-icell', props.large && 'bae-m-icell-large', props.quiet && 'bae-m-icell-quiet'), defaultValue: props.value || '', placeholder: props.placeholder, 'aria-label': props.label, spellCheck: false });
  }
  // A ruled heading over a part of a form: Release, Images, Files, Credits.
  function MacFormHeading(props) { return h('div', { className: 'bae-m-formhead' }, props.children); }
  // The release editor's top row: Identify with Automatic, Search and the Metadata menu (⋯), then the action row: for `actions` `import`, where the import goes (`cloud`, `to`) and Import, or Retry import (`retry`), or neither (`blank`); for `save`, Cancel once something changed (`changed`) and Save.
  function MacIdentifyBar(props) {
    var save = props.actions === 'save';
    return h('div', { className: 'bae-m-idraft-actions' },
      h('span', { className: 'bae-strong bae-muted' }, 'Identify'), h(MacButton, null, 'Automatic'), h(MacButton, null, 'Search'), h(MacButton, { menu: true, label: 'Metadata' }, '⋯'),
      h('span', { className: 'bae-m-idraft-trail' },
      props.cloud && !save ? h(MacImportDestination, { to: props.to }) : null,
      // `reading`: the line a save or an import shows while it reads a file on this device before it writes (the audio file a CUE sheet is newly bound to), before the buttons, which are off until the read ends.
      props.reading ? h('span', { key: 'r', className: 'bae-m-idraft-reading bae-muted' }, h(MacSpinner, { size: 14 }), props.reading) : null,
      save ? [props.changed ? h(MacButton, { key: 'c', off: !!props.reading }, 'Cancel') : null, h(MacButton, { key: 's', kind: 'primary', off: !!props.reading }, 'Save')] : props.blank ? null : h(MacButton, { kind: 'primary', off: !!props.reading }, props.retry ? 'Retry import' : 'Import')));
  }
  // An artist field, a button that opens the artist picker, drawn as its value until the pointer is over it, like MacInlineField: the artists (`artist`) and their chip (`chip`), Library for an artist the library has, New for one the import adds; `placeholder` in the tertiary colour while it names none (Album artist unless given). `inherited`: a track that takes the album's artists shows Album artist in the tertiary colour. `secondary` draws the artists in secondary text, as the album identity does; `column` fills its column, as a track row's does.
  function MacAlbumArtistField(props) {
    var none = props.inherited || !props.artist;
    return h('button', { type: 'button', className: cx('bae-plainbtn', 'bae-m-iartist', props.column && 'bae-m-iartist-column', props.secondary && 'bae-muted') },
      none ? h('span', { className: 'bae-m-tertiary' }, props.inherited ? 'Album artist' : props.placeholder || 'Album artist')
        : [h('span', { key: 'a', className: 'bae-ellipsis' }, props.artist), props.chip ? h(StatusChip, { key: 'c', tone: props.chip === 'New' ? 'accent' : 'neutral' }, props.chip) : null]);
  }
  // The album the release files under (spec §3.7 step 6, §3.8): Album, then the pop-up showing the album as it stands (`album`): New album, the library album its records name, or one picked. `open` draws its menu: the album as it stands, New album when that album is the library's (`inLibrary`), Use the release’s album when the person picked another (`picked`), a line, then Choose another album…, which searches the library's albums.
  function MacAlbumPicker(props) {
    var items = [props.album].concat(props.inLibrary ? ['New album'] : [], props.picked ? ['Use the release’s album'] : [], ['-', 'Choose another album…']).join(',');
    return h('div', { className: 'bae-m-idraft-album' }, h('span', { className: 'bae-muted' }, 'Album'), h(MacPopUpButton, { value: props.album, label: 'Album', open: props.open, items: items }));
  }
  // The release's cover (a plain tile when it has none) beside its identity: the album title and year as fields and the album artist; then what the card it sits in puts under them, its children: the folder's audio line and the album picker.
  function MacReleaseIdentity(props) {
    return h('div', { className: 'bae-m-idraft-head' },
      h(Artwork, { title: props.cover, size: 132, radius: 6, missing: !props.cover }),
      h('div', { className: 'bae-m-idraft-id' },
        h(MacInlineField, { label: 'Album title', value: props.title, placeholder: 'Album title', large: true }),
        h(MacAlbumArtistField, { artist: props.artist, chip: props.artistChip, secondary: true }),
        h(MacInlineField, { label: 'Album year', value: props.year, placeholder: 'Album year', quiet: true }),
        kids(props.children)));
  }
  // The folder's audio as the scan read it (spec §3.6: codec, sample rate, bit depth or bitrate, channels), the identity's line under the year.
  function audioLine(audio) { return audio ? h('div', { key: 'audio', className: 'bae-m-tertiary bae-m-idraft-audio' }, audio) : null; }
  // What Discogs says about the pressing that no other field holds: each detail with its remove button, then the menu that adds one more, showing — while there are none and + Add after.
  function discogsDetails(details) {
    return h('span', { className: 'bae-m-idraft-details' },
      details.map(function (d) { return h('span', { key: d, className: 'bae-m-idraft-detail' }, d, h('button', { type: 'button', className: 'bae-plainbtn bae-m-tertiary', 'aria-label': 'Remove' }, h(Sym, { name: 'xcircle', size: 12 }))); }),
      h('button', { type: 'button', className: cx('bae-plainbtn', 'bae-m-idraft-detail', !details.length && 'bae-m-tertiary'), 'aria-haspopup': 'menu', 'aria-label': 'Details' }, details.length ? [h(Glyph, { key: 'p', set: 'mac', name: 'add', size: 11 }), 'Add'] : '—'));
  }
  // The release's fields under a Release heading: Year, Media, Label with its catalog number, Country, Barcode, Status, Packaging and Details, the typed ones as fields and the chosen ones as pop-ups, each showing — while empty.
  function MacReleaseFields(props) {
    var dash = function (v) { return v || '—'; };
    var field = function (label, value) { return h(MacInlineField, { label: label, value: value, placeholder: '—' }); };
    var grid = [
      ['Year', field('Year', props.year)],
      ['Media', h(MacPopUpButton, { label: 'Media', value: dash(props.media) })],
      ['Label', h('span', { className: 'bae-m-idraft-labelrow' }, field('Label', props.labelName), field('Catalog number', props.catalog))],
      ['Country', h(MacPopUpButton, { label: 'Country', value: dash(props.country) })],
      ['Barcode', field('Barcode', props.barcode)],
      ['Status', h(MacPopUpButton, { label: 'Status', value: dash(props.status) })],
      ['Packaging', h(MacPopUpButton, { label: 'Packaging', value: dash(props.packaging) })],
      ['Details', discogsDetails(list(props.details))]];
    return h('div', { className: 'bae-m-relfields' },
      h(MacFormHeading, null, 'Release'),
      h('div', { className: 'bae-m-idraft-grid' }, grid.map(function (r, i) { return [h('span', { key: 'l' + i, className: 'bae-muted' }, r[0]), h('span', { key: 'v' + i, className: 'bae-m-idraft-cell' }, r[1])]; })));
  }
  // The release's identity and its fields, as both cards draw them under their lead: the cover and identity with the audio line and the album picker, then the Release grid.
  function releaseHeader(props) {
    return [
      h(MacReleaseIdentity, { key: 'id', cover: props.cover, title: props.title, artist: props.artist, artistChip: props.artistChip, year: props.year },
        audioLine(props.audio),
        props.album ? h(MacAlbumPicker, { key: 'album', album: props.album, inLibrary: props.albumInLibrary, picked: props.albumPicked, open: props.albumOpen }) : null),
      h(MacReleaseFields, { key: 'fields', year: props.pressingYear, media: props.media, labelName: props.labelName, catalog: props.catalog, country: props.country, barcode: props.barcode, status: props.status, packaging: props.packaging, details: props.details })];
  }
  // The release editor, one component in the import pane and in the album's Edit release sheet; its `actions` row is the only difference. `actions` `import`: where the import goes (`cloud`, `to`) and Import, or Retry import (`retry`), or neither while the folder has nothing to import (`blank`), with the folder's `check` in a warning notice under that row. `actions` `save`: Cancel once something changed (`changed`), and Save. `reading`: the file a save or an import reads before it writes. Then, in both: the release's identity and fields; the records under a rule, with Unlink; the folder's or release's images (`images`); the track table, the children (side headings, CUE captions and track rows); the Files list (`files`).
  function MacReleaseEditor(props) {
    var tracks = kids(props.children);
    return h('div', { className: 'bae-m-reditor' },
      h('div', { className: 'bae-m-card bae-m-idraft' },
        h(MacIdentifyBar, { actions: props.actions, blank: props.blank, retry: props.retry, cloud: props.cloud, to: props.to, changed: props.changed, reading: props.reading }),
        props.check && props.actions !== 'save' ? h(MacNotice, { tone: 'warning', title: props.check }) : null,
        releaseHeader(props),
        props.records ? h('div', { className: 'bae-m-idraft-records' }, h(MacRecordLinks, { records: props.records }), h('span', { style: { flex: 1 } }), h('button', { type: 'button', className: 'bae-plainbtn bae-accent' }, 'Unlink')) : null),
      props.images ? h(MacImportImages, { images: props.images }) : null,
      tracks.length ? h(MacImportTable, null, tracks) : null,
      props.files ? h(MacReleaseFiles, { files: props.files }) : null);
  }
  // A skipped folder's card, read-only: why bae can't import it (`reason`) in a warning notice where Import would be, then its cover and what its tags call the release (`title`, `artist`), or its `folder` name when they call it nothing.
  function MacInvalidCandidateCard(props) {
    return h('div', { className: 'bae-m-card bae-m-idraft' },
      h(MacNotice, { tone: 'warning', title: props.reason }),
      h('div', { className: 'bae-m-idraft-head' },
        h(Artwork, { title: props.cover, size: 132, radius: 6, missing: !props.cover }),
        props.title ? h(MacImportReleaseSummary, { title: props.title, artist: props.artist }) : h('span', { className: 'bae-mono' }, props.folder)));
  }
  // The folder's images under the release: a ruled Images heading, then each image at 128pt, a button named by its file that opens the lightbox at it (`images`: "artwork title|file name", comma-separated).
  function MacImportImages(props) {
    return h('div', { className: 'bae-m-ifiles' }, h(MacFormHeading, null, 'Images'),
      h('div', { className: 'bae-m-iimages' }, list(props.images).map(function (t) { var p = t.split('|'); return h('button', { key: t, type: 'button', className: 'bae-plainbtn bae-m-iimage', 'aria-label': p[1] }, h(Artwork, { title: p[0], size: 128, radius: 6 })); })));
  }
  // One file of a release: its name in mono and its size at the right. `kind`: `opens` (the default), a document or a track sheet, a button named by its file that opens it; `audio`, a track that plays, with its Play button before the size; `other`, a file that opens nothing. `damaged`: why the file is damaged, marked in the warning colour; a damaged file opens nothing.
  function MacReleaseFileRow(props) {
    var kind = props.kind || 'opens', opens = kind === 'opens' && !props.damaged;
    return h(opens ? 'button' : 'div', { type: opens ? 'button' : null, className: cx(opens && 'bae-plainbtn', 'bae-m-ifile'), 'aria-label': opens ? props.name : null },
      h('span', { className: 'bae-mono bae-ellipsis' }, props.name),
      props.damaged ? h('span', { className: 'bae-m-ifile-damaged' }, h(StatusGlyph, { tone: 'warning', set: 'mac', size: 12 }), props.damaged) : null,
      kind === 'audio' ? h('button', { type: 'button', className: 'bae-plainbtn bae-muted bae-m-ifile-play', 'aria-label': 'Play' }, h(Sym, { name: 'play', size: 10 })) : null,
      h('span', { className: 'bae-m-tertiary bae-m-ifile-size' }, props.size || ''));
  }
  // The release's files that are no track (a rip log, a track sheet that carves no tracks; never an image bae can show), or on a skipped folder's pane every file but its images: a ruled Files heading, then a row per file (`files`: "name|size[|kind]", comma-separated, where kind is `other`, `audio` or `damaged:why`, and a file with none opens).
  function MacReleaseFiles(props) {
    return h('div', { className: 'bae-m-ifiles' }, h(MacFormHeading, null, 'Files'),
      h('div', null, list(props.files).map(function (f) {
        var p = f.split('|'), kind = p[2] || '', damaged = kind.indexOf('damaged:') === 0;
        return h(MacReleaseFileRow, { key: p[0], name: p[0], size: p[1], kind: damaged ? null : kind || null, damaged: damaged ? kind.slice('damaged:'.length) : null });
      })));
  }
  // The mapping table: Source | Track | Title | Artist | Length, a side's eyebrow, CUE captions and rows.
  function MacImportTable(props) {
    return h('div', { className: 'bae-m-itable-scroll' }, h('div', { className: 'bae-m-itable' }, h('div', { className: 'bae-m-itable-row bae-m-itable-head' }, ['Source', 'Track', 'Title', 'Artist', 'Length'].map(function (t) { return h('span', { key: t, className: 'bae-m-eyebrowtxt' }, t); })), props.children));
  }
  // `start`: a CUE image's track, its source cell where the track starts in the image (the sheet's caption names the image once). `listed`: the length the source lists, after the measured `length` in the warning colour, when the two are over 3 s apart, so the row says what disagrees. The title and the artist are fields; an artist the track inherits from the album leaves its field empty, showing Album artist.
  function MacImportTrackRow(props) {
    return h('div', { className: cx('bae-m-itable-row', 'bae-m-itable-track', props.playing && 'bae-m-itable-playing') },
      h('span', { className: 'bae-m-isrc' }, h('button', { type: 'button', className: cx('bae-plainbtn', props.playing ? 'bae-accent' : 'bae-muted'), 'aria-label': props.playing ? 'Stop' : 'Play' }, h(Sym, { name: props.playing ? 'stop' : 'play', size: 10 })), props.start != null ? h('span', { className: 'bae-num bae-muted' }, props.start) : h('span', { className: 'bae-mono bae-ellipsis' }, props.file)),
      h('span', { className: 'bae-num' }, props.position),
      h('span', { className: 'bae-m-itable-cell' }, h(MacInlineField, { label: 'Title', value: props.title, placeholder: 'Untitled' })),
      h('span', { className: 'bae-m-itable-cell' }, h(MacAlbumArtistField, { artist: props.artist, chip: props.artistChip, inherited: props.inherited, placeholder: 'Artist', column: true })),
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
  // One identifier the folder's files state: its label, then its value in mono, or a dash for one they leave out. `state`: `filled` (the default), `outlined`, `struck` through, or `dash`. `sources`: the catalogs searched for it, each with how many releases it found ("MusicBrainz=2,Discogs=0", comma-separated; a count of … is a search still running).
  function MacIdentifierChip(props) {
    var st = props.state || 'filled';
    return h('span', { className: cx('bae-m-idchip', 'bae-m-idchip-' + st) }, h('span', { className: 'bae-m-tertiary', style: { fontSize: 10.5, fontWeight: 600 } }, props.label), st === 'dash' ? h('span', { className: 'bae-m-idchip-dashmark' }) : h('span', { className: 'bae-mono' }, props.value || ''),
      props.sources ? list(props.sources).map(function (cc, j) { var q = cc.split('='); return h('span', { key: j, className: 'bae-m-capsule' }, q[0], q[1] === '…' ? h('span', { className: 'bae-m-tertiary' }, 'Searching…') : h(StatusChip, { tone: Number(q[1]) > 0 ? 'success' : 'neutral' }, q[1])); }) : null);
  }
  // The identifier chips: "Label|value[|state[|Source=count+Source=count]]", comma-separated.
  function MacIdentifierBand(props) {
    return h('div', { className: 'bae-m-band' }, list(props.chips).map(function (c, i) {
      var p = c.split('|');
      return h(MacIdentifierChip, { key: i, label: p[0], value: p[1], state: p[2], sources: p[3] ? p[3].split('+').join(',') : null });
    }));
  }
  // Every catalog that describes a release, as a wrapped row of links to each catalog's page, in the order core lists them (`records`: catalog names, comma-separated).
  // `buttons`: each is a button that opens the page, as on a found album's card, rather than a link.
  function MacRecordLinks(props) {
    return h('span', { className: 'bae-m-reclinks' }, list(props.records).map(function (s) { return h(props.buttons ? 'button' : 'a', props.buttons ? { key: s, type: 'button', className: 'bae-m-reclink bae-m-reclink-button' } : { key: s, href: '#', className: 'bae-m-reclink' }, s, h('span', { className: 'bae-linkout' }, h(Sym, { name: 'external', size: 12 }))); }));
  }
  // A found album: cover, title, artist · label, a button per catalog that has answered (`sources`) that opens the album on that catalog.
  function MacReleaseCard(props) {
    return h('div', { className: 'bae-m-rcard-head' }, h(Artwork, { title: props.title, size: 44, radius: 6 }), h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.title), h('div', { className: 'bae-muted' }, props.artist + (props.label ? ' · ' + props.label : ''))), props.sources ? h(MacRecordLinks, { records: props.sources, buttons: true }) : null);
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
      return h(React.Fragment, { key: i }, i ? h('span', { className: 'bae-m-tertiary' }, ' / ') : null, p.value, h('span', { className: 'bae-m-tertiary bae-m-prow-catalog' }, ' ' + p.catalog));
    });
  }
  // The values of a sourced fact with the catalog each names, from "1992:MusicBrainz|1993:Discogs".
  function sourcedParts(text) {
    return String(text).split('|').map(function (v) { var at = v.lastIndexOf(':'); return { value: v.slice(0, at), catalog: v.slice(at + 1) }; });
  }
  // A found pressing: its year, labels, catalog number, barcode and badges, then `summary` (" · "-separated facts) and `notes` (the records' notes, a release's disambiguation, with the Notes badge), each fact a MacSourcedValue; In library, dimmed whole, for one the library has; `sources`, the catalogs whose records it holds, only those that have answered; the chevron.
  function MacPressingRow(props) {
    var sv = function (v) { return h(MacSourcedValue, { value: v }); };
    var summary = props.summary ? String(props.summary).split(' · ').map(function (part, i) { return h(React.Fragment, { key: i }, i ? ' · ' : null, sv(part)); }) : null;
    return h('div', { className: cx('bae-m-prow', props.selected && 'bae-m-prow-selected'), style: props.inLibrary ? { opacity: 0.55 } : null },
      h('div', { style: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 2 } },
        h('div', { style: { display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap' } }, props.year ? h('span', { className: 'bae-strong bae-num' }, sv(props.year)) : h('span', { className: 'bae-m-tertiary' }, 'Year unknown'), h('span', { className: 'bae-muted' }, props.labels), props.catalog ? h(StatusChip, null, sv(props.catalog)) : null, props.barcode ? h('span', { className: 'bae-muted bae-num' }, sv(props.barcode)) : null, list(props.badges).map(function (b) { return h(StatusChip, { key: b, tone: 'accent' }, b); }), props.notes ? h('span', { title: String(props.notes).indexOf('|') < 0 ? props.notes : sourcedParts(props.notes).map(function (p) { return p.value + ' ' + p.catalog; }).join(' / ') }, h(StatusChip, { tone: 'accent' }, 'Notes')) : null),
        summary ? h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, summary) : null,
        // `notes`: the records' notes (a release's disambiguation), every record's that states one, each with its catalog when they differ. The Notes badge and this line come from the same records, so they show together.
        props.notes ? h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5, fontStyle: 'italic' } }, sv(props.notes)) : null),
      props.inLibrary ? h('span', { style: { display: 'flex', alignItems: 'center', gap: 4, fontSize: 11.5 }, className: 'bae-m-tertiary' }, h(Sym, { name: 'check', size: 11, className: 'bae-success' }), 'In library') : null,
      // `sources`: the catalogs whose records the pressing holds, only those that have answered.
      props.sources ? h('span', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.sources) : null,
      h(Sym, { name: 'right', size: 11, className: props.selected ? 'bae-accent' : 'bae-m-tertiary' }));
  }
  // One bulk action: its glyph, its name, then how many of the selected folders it applies to (`count`), dimmed when none. `strong` sets the action in the accent, for Import and Combine. A child sits before the count: Import's destination, on Import's own line.
  function MacBulkActionRow(props) {
    var n = props.count;
    return h('div', { className: cx('bae-m-bulkrow', !n && 'bae-m-bulkrow-off') }, h(Sym, { name: props.icon, size: 15, className: props.strong ? 'bae-accent' : 'bae-muted' }), h('span', { className: props.strong ? 'bae-strong' : 'bae-muted', style: { flex: 1 } }, props.label), props.children || null, h(StatusChip, { tone: props.strong ? 'accent' : 'neutral' }, String(n || 0)));
  }
  // Several rows selected: the count, then the actions in groups, each with how many it applies to.
  function MacImportBulk(props) {
    // `extra` sits on the row before its count: Import's destination, on Import's own line.
    var row = function (icon, label, n, strong, extra) { return h(MacBulkActionRow, { key: label, icon: icon, label: label, count: n, strong: strong }, extra); };
    var grp = function (eyebrow, rows) { return h('div', { className: 'bae-m-bulkgrp' }, eyebrow ? h('div', { className: 'bae-m-eyebrowtxt', style: { padding: '8px 12px 2px' } }, eyebrow) : null, rows); };
    var n = Number(props.count || 5);
    return h('div', { className: 'bae-m-bulk' }, h('div', { className: 'bae-m-card', style: { width: 440, boxSizing: 'border-box', padding: 24, display: 'flex', flexDirection: 'column', gap: 12 } },
      h('div', { style: { font: '700 22px var(--font-sans)' } }, n + ' selected'), h('div', { className: 'bae-muted' }, 'Each action applies only to eligible selected folders.'),
      h('div', { className: 'bae-m-bulklist' },
        grp('Import', [row('saveAs', 'Import', props.ready || 3, true, props.cloud ? h(MacImportDestination, { to: props.to }) : null), row('xcircle', 'Cancel import of selected', 0), row('grid', 'Combine as one release', 0, true), row('grid', 'Keep as separate releases', 0)]),
        grp('Metadata', [row('refresh', 'Identify selected', n), row('xcircle', 'Stop identifying selected', 0), row('refresh', 'Retry failed identification', 1), row('edit', 'Reset to file metadata', n), row('trash', 'Clear metadata', n)]),
        grp(null, [row('xcircle', 'Skip selected', n), row('left', 'Restore to Pending', 0)]),
        grp(null, [row('folder', 'Reveal in Finder', n)]))),
      props.running ? h('div', { style: { width: 440, display: 'flex', gap: 12, alignItems: 'flex-end' } }, h('div', { style: { flex: 1 } }, h(MacProgress, { label: props.running, value: 0.4 })), h(MacButton, null, 'Cancel')) : null);
  }
