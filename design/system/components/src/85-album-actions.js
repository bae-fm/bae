  // ==== part: album ====
  // ---- Album actions (macOS) ----
  // The modal host: a scrim over the window, the content clipped to the card radius.
  function MacModal(props) {
    return h('div', { className: 'bae-m-scrim', style: { alignItems: 'center' } }, h('div', { className: 'bae-m-modal', style: { width: Number(props.width || 760), height: props.height ? Number(props.height) : null } }, props.children));
  }
  // A modal's header: title, a second line, its buttons on the right.
  function MacModalHeader(props) {
    return h('div', { className: 'bae-m-modalhead' }, h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: props.large ? 'bae-m-modalhead-large' : 'bae-strong', style: props.large ? null : { fontSize: 15 } }, props.title), props.subtitle ? h('div', { className: 'bae-muted bae-ellipsis', style: { fontSize: 11.5 } }, props.subtitle) : null), props.children);
  }
  // One cover in the cover gallery: its image over its name, with no source line, since the section heading says where it comes from. `image` stands for the picture (the board draws it as artwork seeded by it, the name unless given); `selected` rings the one the preview shows.
  function MacCoverTile(props) {
    return h('div', { className: 'bae-m-ctile' },
      h('div', { className: cx('bae-m-ctile-img', props.selected && 'bae-m-ctile-sel') }, h(Artwork, { title: props.image || props.name, size: 118, radius: 'radius-artwork' })),
      h('div', { className: 'bae-ellipsis bae-m-ctile-name' }, props.name));
  }
  // The cover gallery's preview: the picked image large (`image`, seeded as a tile's), its name, where it comes from (`source`), and its place among the images (`position` of `count`) between the previous and next arrows.
  function MacCoverPreview(props) {
    return h('div', { className: 'bae-m-cpreview' },
      h(Artwork, { title: props.image || props.name, size: 300, radius: 'radius-artwork' }),
      h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.name),
      h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.source),
      h('div', { style: { display: 'flex', gap: 'calc(var(--space-related) + var(--space-line))', alignItems: 'center' } }, h(Sym, { name: 'left', size: 12 }), (props.position || 1) + ' of ' + props.count, h(Sym, { name: 'right', size: 12 })));
  }
  // The cover gallery: Source, sections of tiles, the preview with its count, Use this cover.
  function MacCoverGallery(props) {
    // `current`: the release's cover, named Cover (spec §3.4: a release's gallery names its cover Cover), drawn from the album's picture.
    var tiles = function (names, sel, current) { return h('div', { className: 'bae-m-cgrid' }, list(names).map(function (n, i) { return h(MacCoverTile, { key: i, name: n, image: current ? props.album || 'Low Tide Hours' : n, selected: n === sel }); })); };
    var sec = function (icon, title, names, current) { var n = String(list(names).length); return h('div', { style: { display: 'flex', flexDirection: 'column', gap: 'calc(var(--space-related) + var(--space-line))' } }, h('div', { className: 'bae-strong', style: { fontSize: 15, display: 'flex', gap: 'var(--space-compact)', alignItems: 'center' } }, h(Sym, { name: icon, size: 15 }), title, h('span', { className: 'bae-muted', style: { fontWeight: 400 } }, n)), tiles(names, props.selected, current)); };
    var unlinked = props.remote === 'unlinked';
    return h('div', { className: 'bae-m-cgal' },
      // With no linked release there is nothing to fetch, so no Refresh.
      h(MacModalHeader, { title: props.title || 'Change cover', large: true }, unlinked ? null : h(MacButton, null, 'Refresh'), h(MacButton, null, 'Done')),
      h('div', { className: 'bae-m-cgal-body' },
        h('div', { className: 'bae-m-cgal-left' }, h('div', { style: { display: 'flex', gap: 'var(--space-related)', alignItems: 'center' } }, 'Source', h(MacPopUpButton, { label: 'Source', value: 'All' })),
          sec('photo', 'Current cover', 'Cover', true),
          unlinked ? h('div', null, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, 'No linked release'), h('div', { className: 'bae-muted' }, 'Link a Discogs or MusicBrainz release to browse its artwork.'), h(MacButton, null, 'Find release…')) : sec('globe', 'Remote sources', props.remoteNames || 'Discogs · 1 · Front,Discogs · 2 · Back'),
          sec('folder', 'Release files', props.files || 'cover.jpg,back.jpg,disc.jpg')),
        h('div', { className: 'bae-m-cgal-right' }, h(MacCoverPreview, { name: props.selected || 'Low Tide Hours', source: props.source || 'Release files', position: 1, count: 1 + (unlinked ? 0 : list(props.remoteNames || 'Discogs · 1 · Front,Discogs · 2 · Back').length) + list(props.files || 'cover.jpg,back.jpg,disc.jpg').length }))),
      h('div', { className: 'bae-m-cgal-foot' }, h(MacButton, { kind: 'primary' }, 'Use this cover')));
  }
  // The lightbox's thumbnails of every image (`images`, each drawn as artwork seeded by it), the shown one (`selected`, the first unless given) ringed.
  function MacThumbnailStrip(props) {
    var images = list(props.images), sel = props.selected || images[0];
    return h('div', { className: 'bae-m-lb-strip' }, images.map(function (t, i) { return h('span', { key: i, className: cx('bae-m-lb-thumb', t === sel && 'bae-m-lb-thumb-on') }, h(Artwork, { title: t, size: 56, radius: 'radius-artwork' })); }));
  }
  // The lightbox: the image on the backdrop, close, arrows, then at its foot the shown image's caption (`label`, and `source` under it) over the thumbnail strip (`images`, `selected`), which a lone image goes without.
  function MacLightbox(props) {
    var images = list(props.images || 'Low Tide Hours,Low Tide Hours back,Low Tide Hours disc,Low Tide Hours booklet');
    return h('div', { className: 'bae-m-lightbox' },
      h('div', { className: 'bae-m-lb-top' }, props.browse ? h('span', { className: 'bae-m-lb-pill' }, h(Sym, { name: 'grid', size: 12 }), 'Browse all images') : h('span'), h('span', { className: 'bae-m-lb-close' }, h(Sym, { name: 'close', size: 15 }))),
      h('div', { className: 'bae-m-lb-stage' }, images.length > 1 ? h('span', { className: 'bae-m-lb-arrow' }, h(Sym, { name: 'left', size: 22 })) : h('span'), props.state === 'failed' ? h(MacNotice, { layout: 'centered', quiet: true, stage: true, tone: 'danger', title: 'Could not load the image', action: 'Retry' }) : props.state === 'loading' ? h('div', { className: 'bae-m-lb-msg' }, h(MacSpinner, { size: 28 }), 'Loading the image…') : h(Artwork, { title: props.title || 'Low Tide Hours', size: 420, radius: 0 }), images.length > 1 ? h('span', { className: 'bae-m-lb-arrow' }, h(Sym, { name: 'right', size: 22 })) : h('span')),
      h('div', { className: 'bae-m-lb-foot' },
        h('div', { className: 'bae-m-lb-cap' }, h('div', null, props.label || 'cover.jpg'), h('div', { style: { opacity: 0.7, fontSize: 11.5 } }, props.source || 'Release files')),
        images.length > 1 ? h(MacThumbnailStrip, { images: images.join(','), selected: props.selected }) : null));
  }
  // A release's storage at rest (`state`: local, pinned or cloud): Local, Pinned or In cloud.
  function MacStorageState(props) {
    var s = props.state || 'cloud';
    return h('span', { className: 'bae-m-sband-rest' }, { local: 'Local', pinned: 'Pinned', cloud: 'In cloud' }[s]);
  }
  // The release's Storage sheet, as MacStorageBand draws it: one line, the release's state at rest (Local, Pinned, In cloud) and beside it at most one pill for a change on its way, `MacStorageChip` with `long`: Queued to upload or Uploading (`state`: waiting, uploading; the release is Local), Queued to pin or Pinning (`download`: queued, downloading), Queued to move to This Mac or Moving to This Mac… (`moveToThisDevice`: queued, moving); the moving ones fill with `progress`. A failed pin (Couldn’t pin) or move to This Mac is a danger notice under the line, with Details; an upload held by a missing original (`missing`, the file's name) is a warning notice naming it. What holds a whole queue back shows on the Storage Manager's footer, not here. Then why this Mac can't play it, and the storage actions. Nothing says how many wait ahead or what holds the queue back.
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
      props.why ? h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.why) : null,
      still ? null : h('div', { style: { display: 'flex', gap: 'var(--space-related)', flexWrap: 'wrap' } }, list(props.actions || 'Pin,Move to This Mac…,Export…,Save as…').map(function (a) { return h(MacButton, { key: a }, a); })));
  }

