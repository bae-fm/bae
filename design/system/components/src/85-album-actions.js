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
      h('div', { className: 'bae-m-lb-stage' }, h('span', { className: 'bae-m-lb-arrow' }, h(Sym, { name: 'left', size: 22 })), props.state === 'failed' ? h('div', { className: 'bae-m-lb-msg' }, h(Sym, { name: 'warning', size: 40 }), 'Couldn’t load image', h(MacButton, null, 'Try again')) : props.state === 'loading' ? h('div', { className: 'bae-m-lb-msg' }, h(MacSpinner, { size: 28 }), 'Loading the image…') : h(Artwork, { title: props.title || 'Low Tide Hours', size: 420, radius: 0 }), h('span', { className: 'bae-m-lb-arrow' }, h(Sym, { name: 'right', size: 22 }))),
      h('div', { className: 'bae-m-lb-cap' }, h('div', null, props.label || 'cover.jpg'), h('div', { style: { opacity: 0.7, fontSize: 11.5 } }, props.source || 'Release Files')),
      h('div', { className: 'bae-m-lb-strip' }, ['Low Tide Hours', 'Low Tide Hours back', 'Low Tide Hours disc', 'Low Tide Hours booklet'].map(function (t, i) { return h('span', { key: i, className: cx('bae-m-lb-thumb', i === 0 && 'bae-m-lb-thumb-on') }, h(Artwork, { title: t, size: 56, radius: 6 })); })));
  }
  // The release's Storage sheet, as StorageStatusBand draws it, in its order: the state line (Local, Pinned, In storage, or an upload's phase with its bar, or the phase of files only `waiting` to upload, then the `wait` chip); the download line (`download`: queued, downloading, failed, copying, with `ahead`, `wait` and `error`); why this Mac can't play it; the make-local line (`makeLocal`: queued, making, failed, with `ahead`, `wait`, `bytes`, `fileBytes`, `progress` and `error`); then the storage actions. Files waiting or moving, a download in flight and a make-local show no actions.
  function MacStorageBand(props) {
    var s = props.state || 'cloud';
    var chip = function (tone, label, icon) { return h(StatusChip, { tone: tone }, icon ? h(Sym, { name: icon, size: 10 }) : null, label); };
    var chips = function (parts) { return h('span', { className: 'bae-m-chips' }, parts.filter(Boolean).map(function (x, i) { return h(React.Fragment, { key: i }, x); })); };
    var wait = props.wait ? chip('warning', props.wait) : null;
    var ahead = Number(props.ahead || 0) > 0 ? chip('neutral', props.ahead + ' ahead') : null;
    var err = function (text) { return h('div', { className: 'bae-tone-danger', style: { color: 'var(--danger)' } }, text); };
    var d = props.download, m = props.makeLocal;
    var state = s === 'uploading' ? h('div', { style: { display: 'flex', gap: 8, alignItems: 'center' } }, chip('activity', 'Uploading', 'up'), h('div', { className: 'bae-m-track', style: { flex: 1 } }, h('div', { className: 'bae-accent-bg', style: { width: '40%' } })), h('span', { className: 'bae-muted bae-num' }, '96 MB of 221.2 MB'))
      : s === 'waiting' ? chips([chip('neutral', 'Queued', 'clock'), wait])
      : h('div', { className: 'bae-muted', style: { display: 'flex', gap: 6, alignItems: 'center' } }, h(Sym, { name: { local: 'folder', pinned: 'pin', cloud: 'cloud' }[s], size: 14 }), { local: 'Local', pinned: 'Pinned', cloud: 'In storage' }[s]);
    var download = d === 'queued' ? chips([chip('activity', 'Queued'), ahead, wait])
      : d === 'downloading' ? h('div', { style: { display: 'flex', flexDirection: 'column', gap: 8 } }, h('div', { className: 'bae-muted', style: { display: 'flex', gap: 8, alignItems: 'center' } }, h('span', null, 'Pinning for offline'), h('div', { className: 'bae-m-track', style: { flex: 1 } }, h('div', { className: 'bae-pinning-bg', style: { width: '30%', marginLeft: '25%' } }))), wait ? h('div', null, wait) : null)
      : d === 'failed' ? h('div', { style: { display: 'flex', flexDirection: 'column', gap: 2 } }, h('div', null, chip('danger', 'Download failed')), err(props.error || 'The network connection was lost'))
      : d === 'copying' ? h('div', null, chip('activity', 'Copying here')) : null;
    var makeLocal = m === 'queued' ? chips([chip('activity', 'Queued to make local'), ahead, wait])
      : m === 'making' ? h('div', { style: { display: 'flex', flexDirection: 'column', gap: 8 } }, h('div', { className: 'bae-muted', style: { display: 'flex', gap: 4, alignItems: 'center' } }, h('span', null, 'Making local…'), wait), h(MacTransferReadout, { kind: 'pin', bytes: props.bytes || '120 MB of 356 MB', rate: 'Current file: ' + (props.fileBytes || '12 MB of 34.1 MB'), progress: props.progress || 0.34 }))
      : m === 'failed' ? h('div', { style: { display: 'flex', flexDirection: 'column', gap: 2 } }, h('div', null, chip('danger', 'Couldn’t make local')), err(props.error || 'The disk is full'))
      : null;
    var still = s === 'uploading' || s === 'waiting' || d === 'downloading' || m;
    return h('div', { className: 'bae-m-sband' },
      state,
      download,
      props.why ? h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.why) : null,
      makeLocal,
      still ? null : h('div', { style: { display: 'flex', gap: 8, flexWrap: 'wrap' } }, list(props.actions || 'Pin,Make local…,Export…,Save as…').map(function (a) { return h(MacButton, { key: a }, a); })));
  }
