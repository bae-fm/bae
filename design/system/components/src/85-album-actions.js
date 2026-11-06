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
  // The release's Storage sheet: Storage and Done, the status band, the Files table.
  function MacStorageBand(props) {
    var s = props.state || 'cloud';
    return h('div', { className: 'bae-m-sband' },
      s === 'uploading' ? [h('div', { key: 'c' }, h(StatusChip, { tone: 'activity' }, h(Sym, { name: 'up', size: 10 }), 'Uploading')), h('div', { key: 'b', style: { display: 'flex', gap: 8, alignItems: 'center' } }, h('div', { className: 'bae-m-track', style: { flex: 1 } }, h('div', { className: 'bae-accent-bg', style: { width: '40%' } })), h('span', { className: 'bae-muted bae-num' }, '96 MB of 221.2 MB'))]
        : h('div', { className: 'bae-muted', style: { display: 'flex', gap: 6, alignItems: 'center' } }, h(Sym, { name: { local: 'folder', pinned: 'pin', cloud: 'cloud' }[s], size: 14 }), { local: 'Local', pinned: 'Pinned', cloud: 'Cloud' }[s]),
      props.why ? h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.why) : null,
      s === 'uploading' ? null : h('div', { style: { display: 'flex', gap: 8, flexWrap: 'wrap' } }, list(props.actions || 'Pin,Make Local…,Export…,Save As…').map(function (a) { return h(MacButton, { key: a }, a); })));
  }
