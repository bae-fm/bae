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
  // The cover gallery: Source, sections of tiles, the preview with its count, Use This Cover.
  function MacCoverGallery(props) {
    var tiles = function (names, sel) { return h('div', { className: 'bae-m-cgrid' }, list(names).map(function (n, i) { return h('div', { key: i, className: 'bae-m-ctile' }, h('div', { className: cx('bae-m-ctile-img', n === sel && 'bae-m-ctile-sel') }, h(Artwork, { title: n, size: 118, radius: 6 })), h('div', { className: 'bae-ellipsis', style: { fontWeight: 500 } }, n), h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.source || 'Release Files')); })); };
    var sec = function (icon, title, n, names) { return h('div', { style: { display: 'flex', flexDirection: 'column', gap: 10 } }, h('div', { className: 'bae-strong', style: { fontSize: 15, display: 'flex', gap: 6, alignItems: 'center' } }, h(Sym, { name: icon, size: 15 }), title, h('span', { className: 'bae-muted', style: { fontWeight: 400 } }, n)), tiles(names, props.selected)); };
    return h('div', { className: 'bae-m-cgal' },
      h(MacModalHeader, { title: props.title || 'Change Cover', large: true }, h(MacButton, null, 'Refresh'), h(MacButton, null, 'Done')),
      h('div', { className: 'bae-m-cgal-body' },
        h('div', { className: 'bae-m-cgal-left' }, h('div', { style: { display: 'flex', gap: 8, alignItems: 'center' } }, 'Source', h(MacPopUpButton, { value: 'All' })),
          sec('photo', 'Current Cover', '1', props.current || 'Low Tide Hours'),
          props.remote === 'unlinked' ? h('div', null, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, 'No linked release'), h('div', { className: 'bae-muted' }, 'Link a Discogs or MusicBrainz release to browse its artwork.'), h(MacButton, null, 'Find release…')) : sec('globe', 'Remote Sources', '2', props.remoteNames || 'Low Tide Hours (Discogs),Low Tide Hours back (Discogs)'),
          sec('folder', 'Release Files', '3', props.files || 'cover.jpg,back.jpg,disc.jpg')),
        h('div', { className: 'bae-m-cgal-right' }, h(Artwork, { title: props.selected || 'Low Tide Hours', size: 300, radius: 6 }), h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.selected || 'Low Tide Hours'), h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.source || 'Release Files'), h('div', { style: { display: 'flex', gap: 10, alignItems: 'center' } }, h(Sym, { name: 'left', size: 12 }), '1 of 6', h(Sym, { name: 'right', size: 12 })))),
      h('div', { className: 'bae-m-cgal-foot' }, h(MacButton, { kind: 'primary' }, 'Use This Cover')));
  }
  // The lightbox: the image on the backdrop, close, arrows, caption, the thumbnail strip.
  function MacLightbox(props) {
    return h('div', { className: 'bae-m-lightbox' },
      h('div', { className: 'bae-m-lb-top' }, props.browse ? h('span', { className: 'bae-m-lb-pill' }, h(Sym, { name: 'grid', size: 12 }), 'Browse all images') : h('span'), h('span', { className: 'bae-m-lb-close' }, h(Sym, { name: 'close', size: 15 }))),
      h('div', { className: 'bae-m-lb-stage' }, h('span', { className: 'bae-m-lb-arrow' }, h(Sym, { name: 'left', size: 22 })), props.state === 'failed' ? h(MacNotice, { layout: 'centered', quiet: true, stage: true, tone: 'danger', title: 'Couldn’t load the image', action: 'Retry' }) : props.state === 'loading' ? h('div', { className: 'bae-m-lb-msg' }, h(MacSpinner, { size: 28 }), 'Loading the image…') : h(Artwork, { title: props.title || 'Low Tide Hours', size: 420, radius: 0 }), h('span', { className: 'bae-m-lb-arrow' }, h(Sym, { name: 'right', size: 22 }))),
      h('div', { className: 'bae-m-lb-cap' }, h('div', null, props.label || 'cover.jpg'), h('div', { style: { opacity: 0.7, fontSize: 11.5 } }, props.source || 'Release Files')),
      h('div', { className: 'bae-m-lb-strip' }, ['Low Tide Hours', 'Low Tide Hours back', 'Low Tide Hours disc', 'Low Tide Hours booklet'].map(function (t, i) { return h('span', { key: i, className: cx('bae-m-lb-thumb', i === 0 && 'bae-m-lb-thumb-on') }, h(Artwork, { title: t, size: 56, radius: 6 })); })));
  }
  // The release's Storage sheet, as StorageStatusBand draws it: one line, the release's state at rest (Local, Pinned, In cloud) and beside it at most one pill for a change on its way: Queued to upload or Uploading (`state`: waiting, uploading; the release is Local), Queued to pin or Pinning (`download`: queued, downloading, copying), Queued to make local or Making local… (`makeLocal`: queued, making); the moving ones fill with `progress`. A failed pin (Couldn't pin) or make-local is a danger notice under the line, with Details; an upload held by a missing original (`missing`, the file's name) is a warning notice naming it. What holds a whole queue back shows on the Storage Manager's footer, not here. Then why this Mac can't play it, and the storage actions. Nothing says how many wait ahead or what holds the queue back.
  function MacStorageBand(props) {
    var s = props.state || 'cloud';
    var d = props.download, m = props.makeLocal;
    var rest = s === 'uploading' || s === 'waiting' ? 'local' : s;
    var restLabel = h('span', { className: 'bae-m-sband-rest' }, h(Sym, { name: { local: 'folder', pinned: 'pin', cloud: 'cloud' }[rest], size: 14 }), { local: 'Local', pinned: 'Pinned', cloud: 'In cloud' }[rest]);
    var pill = s === 'uploading' ? h(MacStorageChip, { state: 'uploading', progress: props.progress })
      : s === 'waiting' ? h(MacStorageChip, { state: 'uploadQueued' })
      : d === 'queued' ? h(MacStorageChip, { state: 'queued' })
      : d === 'downloading' || d === 'copying' ? h(MacStorageChip, { state: 'pinning', progress: props.progress })
      : m === 'queued' ? h(MacStorageChip, { state: 'makeLocalQueued' })
      : m === 'making' ? h(MacStorageChip, { state: 'detaching', progress: props.progress })
      : null;
    var missing = s === 'waiting' && props.missing ? h(MacNotice, { tone: 'warning', title: props.missing + ' is missing', subtitle: 'Restore it to finish uploading.' }) : null;
    var failed = d === 'failed' ? 'Couldn’t pin' : m === 'failed' ? 'Couldn’t make local' : null;
    var still = s === 'uploading' || s === 'waiting' || d === 'downloading' || m === 'queued' || m === 'making';
    return h('div', { className: 'bae-m-sband' },
      h('div', { className: 'bae-m-sband-line' }, restLabel, pill),
      missing,
      failed ? h(MacNotice, { tone: 'danger', title: failed, secondary: 'Details' }) : null,
      props.why ? h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.why) : null,
      still ? null : h('div', { style: { display: 'flex', gap: 8, flexWrap: 'wrap' } }, list(props.actions || 'Pin,Make local…,Export…,Save as…').map(function (a) { return h(MacButton, { key: a }, a); })));
  }

