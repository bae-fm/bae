  // ==== part: album ====
  // ---- Album actions (macOS) ----
  // The modal host: a scrim over the window, the content clipped to the sheet's face, centred over the whole window as the app's MacModal centres it.
  function MacModal(props) {
    return h('div', { className: 'bae-m-scrim bae-m-scrim-centred' }, h('div', { className: 'bae-m-face-sheet bae-m-modal', style: { width: Number(props.width || 760), height: props.height ? Number(props.height) : null } }, props.children));
  }
  // A modal's header: title, a second line space-line under it, its buttons on the right. The title is heading (title with `large`) and the second line detail, each a line of its token's height, as the app's MacModalHeader draws them.
  function MacModalHeader(props) {
    return h('div', { className: 'bae-m-modalhead' }, h('div', { className: 'bae-m-modalhead-text' }, h('div', { className: props.large ? 'bae-m-modalhead-large' : 'bae-m-modalhead-title' }, props.title), props.subtitle ? h('div', { className: 'bae-muted bae-ellipsis bae-m-modalhead-sub' }, props.subtitle) : null), props.children);
  }
  // One cover in the cover gallery: its image over its name, with no source line, since the section heading says where it comes from. `image` stands for the picture (the board draws it as artwork seeded by it, the name unless given); `selected` rings the one the preview shows.
  // The tile is one button, named by the image's name; the shown one is the current one, which the Mac reads as selected.
  function MacCoverTile(props) {
    return h('button', { type: 'button', className: 'bae-plainbtn bae-m-ctile', 'aria-label': props.name, 'aria-current': props.selected ? 'true' : null },
      h('span', { className: cx('bae-m-ctile-img', props.selected && 'bae-m-ctile-sel') }, h(Artwork, { title: props.image || props.name, size: 118, radius: 'radius-artwork' })),
      h('span', { className: 'bae-ellipsis bae-m-ctile-name' }, props.name));
  }
  // The cover gallery's preview: the picked image large (`image`, seeded as a tile's), a button named View in lightbox that opens it there, its name, where it comes from (`source`), and its place among the images (`position` of `count`, in secondary text) between the Previous image and Next image buttons.
  function MacCoverPreview(props) {
    return h('div', { className: 'bae-m-cpreview' },
      h('button', { type: 'button', className: 'bae-plainbtn', 'aria-label': 'View in lightbox', title: 'View in lightbox' }, h(Artwork, { title: props.image || props.name, size: 300, radius: 'radius-artwork' })),
      h('div', { className: 'bae-strong', style: { fontSize: 'var(--type-heading-size)', lineHeight: 'var(--type-heading-line-height)' } }, props.name),
      h('div', { className: 'bae-muted bae-m-detail' }, props.source),
      h('div', { style: { display: 'flex', gap: 'calc(var(--space-related) + var(--space-line))', alignItems: 'center' } }, h(Glyph, { set: 'mac', name: 'down', point: 'backward', size: 13, weight: 'regular', frame: [12, 16], button: 'Previous image' }), h('span', { className: 'bae-muted bae-num' }, (props.position || 1) + ' of ' + props.count), h(Glyph, { set: 'mac', name: 'down', point: 'forward', size: 13, weight: 'regular', frame: [12, 16], button: 'Next image' })));
  }
  // The cover gallery: Source, sections of tiles, the preview with its count, Use this cover, off while the preview shows the current cover.
  function MacCoverGallery(props) {
    // `current`: the release's cover, named Cover (spec §3.4: a release's gallery names its cover Cover), drawn from the album's picture.
    var tiles = function (names, sel, current) { return h('div', { className: 'bae-m-cgrid' }, list(names).map(function (n, i) { return h(MacCoverTile, { key: i, name: n, image: current ? props.album || 'Low Tide Hours' : n, selected: n === sel }); })); };
    var sec = function (title, names, current) { var n = String(list(names).length); return h('div', { style: { display: 'flex', flexDirection: 'column', gap: 'calc(var(--space-related) + var(--space-line))' } }, h('div', { className: 'bae-strong', style: { fontSize: 'var(--type-heading-size)', lineHeight: 'var(--type-heading-line-height)', display: 'flex', gap: 'var(--space-compact)', alignItems: 'center' } }, title, h('span', { className: 'bae-muted', style: { fontWeight: 'var(--type-body-weight)' } }, n)), tiles(names, sel, current)); };
    var unlinked = props.remote === 'unlinked';
    var remotes = unlinked ? [] : list(props.remoteNames || 'Discogs · 1 · Front,Discogs · 2 · Back');
    var files = list(props.files || 'cover.jpg,back.jpg,disc.jpg');
    var all = ['Cover'].concat(remotes, files);
    // `selected`: the image the preview shows, the release's cover unless given (spec §3.4: the picker opens on the saved cover). The preview names its source as the app does: Library for the cover, the catalog for a catalog's image, Release files for a file.
    var sel = props.selected || 'Cover';
    var source = sel === 'Cover' ? 'Library' : remotes.indexOf(sel) >= 0 ? sel.split(' · ')[0] : 'Release files';
    return h('div', { className: 'bae-m-cgal' },
      // With no linked release there is nothing to fetch, so no Refresh.
      h(MacModalHeader, { title: props.title || 'Change cover', large: true }, unlinked ? null : h(MacButton, null, 'Refresh'), h(MacButton, null, 'Done')),
      h('div', { className: 'bae-m-cgal-body' },
        h('div', { className: 'bae-m-cgal-left' }, h('div', { style: { display: 'flex', gap: 'var(--space-related)', alignItems: 'center' } }, 'Source', h(MacPopUpButton, { label: 'Source', value: 'All' })),
          sec('Current cover', 'Cover', true),
          unlinked ? h('div', { style: { display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 'var(--space-related)' } }, h('div', { className: 'bae-strong', style: { fontSize: 'var(--type-heading-size)', lineHeight: 'var(--type-heading-line-height)' } }, 'No linked release'), h('div', { className: 'bae-muted' }, 'Link a Discogs or MusicBrainz release to browse its artwork.'), h(MacButton, null, 'Find release…')) : sec('Remote sources', remotes.join(',')),
          sec('Release files', files.join(','))),
        h('div', { className: 'bae-m-cgal-right' }, h(MacCoverPreview, { name: sel, image: sel === 'Cover' ? props.album || 'Low Tide Hours' : sel, source: source, position: all.indexOf(sel) + 1, count: all.length }))),
      // Use this cover is off while the preview shows the release's cover, which is the cover already.
      h('div', { className: 'bae-m-cgal-foot' }, h(MacButton, { kind: 'primary', off: sel === 'Cover' }, 'Use this cover')));
  }
  // The lightbox's thumbnails of every image (`images`, each drawn as artwork seeded by it), the shown one (`selected`, the first unless given) ringed. Each thumbnail is a button that shows its image, named by the caption the lightbox shows for it (`names`, in the images' order; the image's seed where none is given).
  function MacThumbnailStrip(props) {
    var images = list(props.images), names = list(props.names), sel = props.selected || images[0];
    return h('div', { className: 'bae-m-lb-strip' }, images.map(function (t, i) { return h('button', { key: i, type: 'button', 'aria-label': names[i] || t, className: cx('bae-plainbtn bae-m-lb-thumb', t === sel && 'bae-m-lb-thumb-on') }, h(Artwork, { title: t, size: 'size-thumbnail', radius: 'radius-artwork' })); }));
  }
  // The lightbox: the image on the backdrop, as large as the stage holds, the Close button, the Back and Forward buttons, then at its foot the shown image's caption (`label`, and `source` under it) over the thumbnail strip (`images`, `names`, `selected`), which a lone image goes without. `details`: the failure has causes behind it, such as core's error when the picture's tiles could not be read, so the failed notice has Details.
  function MacLightbox(props) {
    var images = list(props.images || 'Low Tide Hours,Low Tide Hours back,Low Tide Hours disc,Low Tide Hours booklet');
    // The default images' captions, which name their thumbnails.
    var names = props.names || (props.images ? null : 'cover.jpg,back.jpg,disc.jpg,booklet.jpg');
    return h('div', { className: 'bae-m-lightbox' },
      h('div', { className: 'bae-m-lb-top' }, props.browse ? h('button', { type: 'button', className: 'bae-plainbtn bae-m-lb-pill' }, 'Browse all images') : h('span'), h('button', { type: 'button', className: 'bae-plainbtn bae-m-lb-close', 'aria-label': 'Close' }, h(Glyph, { set: 'mac', name: 'close', size: 'stageClose' }))),
      h('div', { className: 'bae-m-lb-stage' }, images.length > 1 ? h('button', { type: 'button', className: 'bae-plainbtn bae-m-lb-arrow', 'aria-label': 'Back' }, h(Glyph, { set: 'mac', name: 'down', point: 'backward', size: 'stageArrow' })) : h('span'), props.state === 'failed' ? h(MacNotice, { layout: 'centered', quiet: true, stage: true, tone: 'danger', title: 'Could not load the image', secondary: props.details ? 'Details' : null, action: 'Retry' }) : props.state === 'loading' ? h(MacSpinner, { label: 'Loading the image…' }) : h('div', { className: 'bae-m-lb-picture' }, h('div', { className: 'bae-m-lb-fit' }, h(Artwork, { title: props.title || 'Low Tide Hours', radius: 0 }))), images.length > 1 ? h('button', { type: 'button', className: 'bae-plainbtn bae-m-lb-arrow', 'aria-label': 'Forward' }, h(Glyph, { set: 'mac', name: 'down', point: 'forward', size: 'stageArrow' })) : h('span')),
      h('div', { className: 'bae-m-lb-foot' },
        h('div', { className: 'bae-m-lb-cap' }, h('div', null, props.label || 'cover.jpg'), h('div', { className: 'bae-m-detail', style: { opacity: 0.7 } }, props.source || 'Release files')),
        images.length > 1 ? h(MacThumbnailStrip, { images: images.join(','), names: names, selected: props.selected }) : null));
  }
  // A release's storage at rest (`state`: local, pinned or cloud): Local, Pinned or In cloud.
  function MacStorageState(props) {
    var s = props.state || 'cloud';
    return h('span', { className: 'bae-m-sband-rest' }, { local: 'Local', pinned: 'Pinned', cloud: 'In cloud' }[s]);
  }
  // The release's Storage sheet, as MacStorageBand draws it: one line, the release's state at rest (Local, Pinned, In cloud) and beside it at most one pill for a change on its way, `MacStorageChip` with `long`: Queued to upload or Uploading (`state`: waiting, uploading; the release is Local), Queued to pin or Pinning (`download`: queued, downloading), Queued to move to This Mac or Moving to This Mac… (`moveToThisDevice`: queued, moving); the moving ones fill with `progress`. A failed pin (Couldn’t pin) or move to This Mac is a danger notice under the line, with Details; an upload held by a missing original (`missing`, the file's name) is a warning notice naming it. What holds a whole queue back shows on the Storage Manager's footer, not here. Then why this Mac can't play it, and the storage actions: none while something moves; Cancel pinning in Pin's place while a pin is queued. Nothing says how many wait ahead or what holds the queue back.
  function MacStorageBand(props) {
    var s = props.state || 'cloud';
    var d = props.download, m = props.moveToThisDevice;
    var rest = s === 'uploading' || s === 'waiting' ? 'local' : s;
    var restLabel = h(MacStorageState, { state: rest });
    var pill = s === 'uploading' ? h(MacStorageChip, { state: 'uploading', progress: props.progress, long: true })
      : s === 'waiting' ? h(MacStorageChip, { state: 'uploadQueued', long: true })
      : d === 'queued' ? h(MacStorageChip, { state: 'queued', long: true })
      : d === 'downloading' ? h(MacStorageChip, { state: 'pinning', progress: props.progress, long: true })
      : m === 'queued' ? h(MacStorageChip, { state: 'moveToThisDeviceQueued', long: true })
      : m === 'moving' ? h(MacStorageChip, { state: 'detaching', progress: props.progress, long: true })
      : null;
    var missing = s === 'waiting' && props.missing ? h(MacNotice, { tone: 'warning', title: props.missing + ' is missing', subtitle: 'Restore it to finish uploading.' }) : null;
    var failed = d === 'failed' ? 'Could not pin' : m === 'failed' ? 'Could not move to This Mac' : null;
    var still = s === 'uploading' || s === 'waiting' || d === 'downloading' || m === 'queued' || m === 'moving';
    return h('div', { className: 'bae-m-sband' },
      h('div', { className: 'bae-m-sband-line' }, restLabel, pill),
      missing,
      failed ? h(MacNotice, { tone: 'danger', title: failed, secondary: 'Details' }) : null,
      props.why ? h('div', { className: 'bae-muted bae-m-detail' }, props.why) : null,
      still ? null : h('div', { style: { display: 'flex', gap: 'var(--space-related)', flexWrap: 'wrap' } }, list(props.actions || 'Pin,Move to This Mac…,Export…,Save as…').map(function (a) { return d === 'queued' && a === 'Pin' ? 'Cancel pinning' : a; }).map(function (a) { return h(MacButton, { key: a }, a); })));
  }

