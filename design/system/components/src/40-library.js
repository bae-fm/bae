  // ---- Library and playback ----

  // A stand-in cover: each title gets its own two-color square, so boards read like a library.
  // `missing` is a cover the release doesn't have: a plain tile, no glyph.
  function hueOf(s) { var x = 0; s = String(s || ''); for (var i = 0; i < s.length; i++) x = (x * 31 + s.charCodeAt(i)) % 360; return x; }
  function Artwork(props) {
    var size = props.size != null ? Number(props.size) : null;
    var hue = props.hue != null ? Number(props.hue) : hueOf(props.seed || props.title);
    var style = { borderRadius: Number(props.radius != null ? props.radius : 10) };
    if (size) { style.width = size; style.height = size; }
    if (props.round) style.borderRadius = '50%';
    if (!props.missing) style.backgroundImage = 'linear-gradient(135deg, hsl(' + hue + ' 42% 52%), hsl(' + ((hue + 48) % 360) + ' 46% 24%))';
    return h('div', { className: cx('bae-art', props.missing && 'bae-art-missing', props.shadow && 'bae-art-shadow-' + props.shadow, props.ring && 'bae-art-ring', props.className), style: style, role: 'img', 'aria-label': props.label || props.title || 'Cover' }, props.children);
  }

  var SF = {
    play: 'M8 5v14l11-7z', pause: 'M6 19h4V5H6v14zm8-14v14h4V5h-4z',
    previous: 'M11 18V6l-8.5 6 8.5 6zm.5-6l8.5 6V6l-8.5 6z', next: 'M4 18l8.5-6L4 6v12zm9-12v12l8.5-6L13 6z',
    shuffle: 'M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z',
    repeat: 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z',
    queue: 'M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z',
    speaker: 'M3 9v6h4l5 5V4L7 9H3zm13.5 3A4.5 4.5 0 0014 7.97v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z',
    cast: 'M7 2h10a2 2 0 012 2v16a2 2 0 01-2 2H7a2 2 0 01-2-2V4a2 2 0 012-2zm5 2.5a1.75 1.75 0 100 3.5 1.75 1.75 0 000-3.5zm0 5.5a4.5 4.5 0 100 9 4.5 4.5 0 000-9zm0 2.25a2.25 2.25 0 110 4.5 2.25 2.25 0 010-4.5z',
    gear: 'M19.14 12.94a7.07 7.07 0 000-1.88l2.03-1.58a.5.5 0 00.12-.61l-1.92-3.32a.5.5 0 00-.6-.22l-2.39.96a7 7 0 00-1.62-.94l-.36-2.54A.5.5 0 0013.9 2.5h-3.84a.5.5 0 00-.49.42l-.36 2.54c-.59.24-1.13.56-1.62.94l-2.39-.96a.5.5 0 00-.6.22L2.68 8.98a.5.5 0 00.12.61l2.03 1.58a7.07 7.07 0 000 1.88l-2.03 1.58a.5.5 0 00-.12.61l1.92 3.32c.13.22.39.3.6.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.25.42.49.42h3.84c.24 0 .45-.18.49-.42l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .6-.22l1.92-3.32a.5.5 0 00-.12-.61l-2.03-1.58zM12 15.6a3.6 3.6 0 110-7.2 3.6 3.6 0 010 7.2z',
    search: 'M15.5 14h-.79l-.28-.27A6.47 6.47 0 0016 9.5 6.5 6.5 0 109.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z',
    ellipsis: 'M6 10a2 2 0 100 4 2 2 0 000-4zm6 0a2 2 0 100 4 2 2 0 000-4zm6 0a2 2 0 100 4 2 2 0 000-4z',
    warning: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20zM10.9 6.5h2.2l-.3 8h-1.6zM12 15.8a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 1 0 0-2.6z',
    close: 'M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z',
    check: 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z',
    airplay: 'M6 22h12l-6-6-6 6zM21 3H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h4v-2H3V5h18v12h-4v2h4c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2z',
    tv: 'M21 3H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h5v2h8v-2h5c1.1 0 1.99-.9 1.99-2L23 5c0-1.1-.9-2-2-2zm0 14H3V5h18v12z'
  };
  function Glyph(props) {
    if (props.set !== 'mac') return h(MSymbol, { name: props.name, size: props.size, className: cx('bae-glyph', props.className), fill: 'currentColor' });
    var set = SF;
    var s = Number(props.size || 24);
    return h('svg', { width: s, height: s, viewBox: '0 0 24 24', 'aria-hidden': true, className: cx('bae-glyph', props.className), fill: 'currentColor' }, h('path', { d: set[props.name] || set.ellipsis || '', fillRule: 'evenodd' }));
  }

  // `large` is Android's full player's bar: a tall track with a tall thumb, and no held parts.
  // Position and the parts this device holds (`held`, "0-0.4,0.6-0.7"), drawn faint under what has played.
  function heldRanges(held) {
    if (held == null || held === '') return [];
    if (typeof held === 'number') return [[0, held]];
    return String(held).split(',').map(function (r) { var p = r.split('-'); return [Number(p[0]), Number(p[1])]; });
  }
  function SeekLine(prefix) {
    return function (props) {
      var pos = Number(props.position || 0);
      return h('div', { className: cx('bae-seek', prefix + '-seek', props.off && 'bae-seek-off', props.large && prefix + '-seek-large') },
        h('span', { className: 'bae-seek-time' }, props.elapsed || '0:00'),
        h('div', { className: 'bae-seek-track', role: 'slider', 'aria-label': 'Position', 'aria-valuenow': Math.round(pos * 100) },
          heldRanges(props.held).map(function (r, i) { return h('div', { key: i, className: 'bae-seek-held', style: { left: (r[0] * 100) + '%', width: ((r[1] - r[0]) * 100) + '%' } }); }),
          props.off ? null : h('div', { className: 'bae-seek-played', style: { width: (pos * 100) + '%' } }),
          prefix === 'bae-a' && !props.off ? h('div', { className: 'bae-seek-thumb', style: { left: (pos * 100) + '%' } }) : null),
        h('span', { className: 'bae-seek-time' }, props.duration || '0:00'));
    };
  }
  var MacSeekBar = SeekLine('bae-m');
  var AndroidSeekBar = SeekLine('bae-a');

  // ---- macOS library ----

  function MacIconButton(props) {
    return h('button', { type: 'button', className: cx('bae-m-iconbtn', props.active && 'bae-m-iconbtn-on', props.faint && 'bae-m-iconbtn-faint', props.small && 'bae-m-iconbtn-small', props.boxed && 'bae-m-iconbtn-boxed'), 'aria-label': props.label || props.icon, title: props.label },
      h(Glyph, { set: 'mac', name: props.icon, size: props.size || 17 }));
  }

  function MacPlayPause(props) {
    return h('span', { className: 'bae-m-playpause' },
      props.waiting ? h('svg', { className: 'bae-waitarc', width: 48, height: 48, viewBox: '0 0 48 48', 'aria-label': 'Downloading the track…' }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })) : null,
      h(Glyph, { set: 'mac', name: props.playing ? 'pause' : 'play', size: 20 }));
  }

  function MacNowPlayingBar(props) {
    var none = !props.title;
    return h('div', { className: 'bae-m-npbar' },
      h('div', { className: 'bae-m-np-left' }, none ? null : [
        h(Artwork, { key: 'a', title: props.album || props.title, size: 56, radius: 6, shadow: 'bar', missing: props.noCover }),
        h('div', { key: 't', className: 'bae-m-np-text' }, h('div', { className: 'bae-m-np-title' }, props.title), h('div', { className: 'bae-m-np-sub' }, props.artist))]),
      h('div', { className: 'bae-m-np-center' },
        h('div', { className: 'bae-m-np-transport' },
          h(MacIconButton, { icon: 'shuffle', label: 'Shuffle', active: props.shuffle }),
          h(MacIconButton, { icon: 'previous', label: 'Previous' }),
          h(MacPlayPause, { playing: props.playing, waiting: props.waiting }),
          h(MacIconButton, { icon: 'next', label: 'Next' }),
          h(MacIconButton, { icon: 'repeat', label: 'Repeat', active: props.repeat })),
        h(MacSeekBar, { position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, off: none })),
      h('div', { className: 'bae-m-np-right' },
        h(MacIconButton, { icon: 'cast', label: props.casting ? 'Casting to ' + props.casting : 'Cast', active: !!props.casting }),
        h(MacIconButton, { icon: 'queue', label: 'Queue', active: props.queueOpen }),
        h(MacIconButton, { icon: 'speaker', label: 'Mute' }),
        h('div', { className: 'bae-m-volume' }, h('div', { style: { width: ((props.volume != null ? Number(props.volume) : 0.7) * 100) + '%' } }))));
  }

  function MacMainWindow(props) {
    var section = props.section || 'Library';
    return h('div', { className: 'bae-m-window bae-m-main-window', style: { width: Number(props.width || 1200), height: Number(props.height || 760) } },
      h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
      h('header', { className: 'bae-m-titlebar' },
        h('div', { className: 'bae-m-sections', role: 'tablist' }, ['Library', 'Import'].map(function (s) {
          return h('span', { key: s, role: 'tab', 'aria-selected': s === section, className: cx('bae-m-section-tab', s === section && 'bae-m-section-tab-on') }, s);
        })),
        h('div', { className: 'bae-m-titlebar-trail' },
          h('div', { className: cx('bae-m-search', props.searchFocused && 'bae-m-search-focused') }, h(Glyph, { set: 'mac', name: 'search', size: 14 }), h('span', { className: props.search ? null : 'bae-m-search-prompt' }, props.search || 'Search')),
          props.sync === 'moving' ? h('span', { className: 'bae-m-iconbtn', title: 'Activity' }, h(MacSpinner, { size: 14 })) : null,
          props.sync === 'attention' ? h('span', { className: 'bae-m-iconbtn bae-m-sync-warn', title: 'Activity' }, h(Glyph, { set: 'mac', name: 'warning', size: 17 })) : null,
          // A queue held (Paused, Waiting for network) with nothing moving: the button stays, as a pause symbol, so the hold can be seen.
          props.sync === 'paused' ? h(MacIconButton, { icon: 'pause', label: 'Activity' }) : null,
          h(MacIconButton, { icon: 'gear', label: 'Settings' }))),
      h('div', { className: 'bae-m-main-body' }, h('div', { className: 'bae-m-main-content' }, kids(props.children).filter(function (c) { var p = inner(c); return !(p && p.type === MacQueuePanel); })), kids(props.children).filter(function (c) { var p = inner(c); return p && p.type === MacQueuePanel; })),
      h(MacNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, playing: props.playing, waiting: props.waiting, position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, shuffle: props.shuffle, repeat: props.repeat, casting: props.casting, queueOpen: props.queueOpen }));
  }

  function MacLibraryHeading(props) {
    var collapsed = !!props.collapsed;
    return h('div', { className: cx('bae-m-libhead', collapsed && 'bae-m-libhead-collapsed') },
      h('h1', { className: 'bae-m-libhead-title' }, props.view || 'Albums', h('span', { className: 'bae-m-libhead-chevron', 'aria-hidden': true }, h(Glyph, { set: 'mac', name: 'down', size: collapsed ? 8 : 15 }))),
      h('div', { className: 'bae-m-libhead-trail' }, props.children));
  }
  SF.down = 'M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6 1.41-1.41z';

  function MacAlbumGrid(props) {
    var n = Number(props.columns || 4);
    return h('div', { className: 'bae-m-grid', style: { gridTemplateColumns: 'repeat(' + n + ', minmax(0, 1fr))' } }, props.children);
  }

  function MacAlbumTile(props) {
    return h('div', { className: cx('bae-m-tile', props.selected && 'bae-m-tile-selected') },
      h('div', { className: 'bae-m-tile-art' },
        h(Artwork, { title: props.title, missing: props.noCover, radius: 10, shadow: 'tile', ring: props.open }),
        props.hover ? h('span', { className: 'bae-m-tile-menu', 'aria-label': 'Album options' }, h(Glyph, { set: 'mac', name: 'ellipsis', size: 13 })) : null),
      h('div', { className: 'bae-m-tile-title' }, props.title),
      h('div', { className: 'bae-m-tile-artist' }, props.artist),
      h('div', { className: 'bae-m-tile-year' }, props.year || ' '));
  }

  function MacTrackRow(props) {
    var lead = props.hover ? h(Glyph, { set: 'mac', name: props.current && props.playing ? 'pause' : 'play', size: 13 })
      : props.current ? h(Glyph, { set: 'mac', name: 'speaker', size: 11, className: 'bae-accent' })
      : h('span', { className: 'bae-m-track-pos' }, props.position);
    return h('div', { className: cx('bae-m-track', props.hover && 'bae-m-track-hover', props.artist && 'bae-m-track-two') },
      h('span', { className: 'bae-m-track-lead' }, lead),
      h('div', { className: 'bae-m-track-text' },
        h('div', { className: cx('bae-m-track-title', props.current && 'bae-accent') }, props.title),
        props.artist ? h('div', { className: 'bae-m-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-m-track-time' }, props.duration));
  }

  function MacEyebrow(props) { return h('div', { className: 'bae-m-eyebrow' }, props.children); }

  function MacBrowseRow(props) {
    if (props.loading) return h('div', { className: 'bae-m-browse bae-m-browse-loading' }, h('div', { className: 'bae-m-browse-art bae-ph' }), h('div', { className: 'bae-m-browse-text' }, h('div', { className: 'bae-ph', style: { width: 140, height: 11 } }), h('div', { className: 'bae-ph', style: { width: 80, height: 10 } })));
    return h('div', { className: cx('bae-m-browse', props.selected && 'bae-m-browse-selected', props.hover && 'bae-m-browse-hover') },
      h(Artwork, { title: props.name, size: 44, radius: 6, missing: props.noImage }),
      h('div', { className: 'bae-m-browse-text' }, h('div', { className: 'bae-m-browse-name' }, props.name), h('div', { className: 'bae-m-browse-count' }, props.count)));
  }

  function MacDetailHeader(props) {
    return h('div', { className: 'bae-m-dethead' },
      h(Artwork, { title: props.name, size: 72, radius: 10, missing: props.noImage }),
      h('div', null, h('div', { className: 'bae-m-dethead-name' }, props.name), h('div', { className: 'bae-m-dethead-count' }, props.count)));
  }

  function MacQueueRow(props) {
    return h('div', { className: cx('bae-m-qrow', props.hover && 'bae-m-qrow-hover') },
      h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }, props.hover ? h('span', { className: 'bae-m-qrow-play' }, h(Glyph, { set: 'mac', name: 'play', size: 11 })) : null),
      h('div', { className: 'bae-m-qrow-text' }, h('div', { className: 'bae-m-qrow-title' }, props.title), h('div', { className: 'bae-m-qrow-sub' }, props.artist), h('div', { className: 'bae-m-qrow-sub' }, props.album)),
      props.hover ? h('span', { className: 'bae-m-qrow-remove', title: 'Remove from queue' }, h(Glyph, { set: 'mac', name: 'close', size: 11 })) : h('span', { className: 'bae-m-qrow-time' }, props.duration));
  }


  // The close control of a docked pane and of an open album: an × in a round, quiet fill.
  function MacCloseButton(props) {
    return h('button', { type: 'button', className: 'bae-m-close', 'aria-label': props.label || 'Close', title: props.label || 'Close' }, h(Glyph, { set: 'mac', name: 'close', size: 13 }));
  }

  // The album opened in the grid: a panel under the album's row.
  function MacAlbumPanel(props) {
    var releases = props.releases ? String(props.releases).split(',').map(function (r) { return r.trim(); }) : null;
    return h('section', { className: 'bae-m-albumpanel' },
      h('span', { className: 'bae-m-albumpanel-close' }, h(MacCloseButton)),
      h(Artwork, { title: props.title, size: Number(props.coverSize || 340), radius: 10, missing: props.noCover, className: 'bae-m-albumpanel-cover' }),
      h('div', { className: 'bae-m-albumpanel-body' },
        h('h2', { className: 'bae-m-albumpanel-title' }, props.title),
        h('div', { className: 'bae-m-albumpanel-sub' }, h('span', { className: 'bae-muted' }, props.artist), props.year ? [h('span', { key: 'd', className: 'bae-m-tertiary' }, '·'), h('span', { key: 'y', className: 'bae-m-tertiary' }, props.year)] : null),
        releases && releases.length > 1 ? h('div', { className: 'bae-m-albumpanel-releases' }, h(MacSegmentedControl, { options: releases.join(','), selected: props.release || releases[0] })) : null,
        props.pressing || props.labels ? h('div', { className: 'bae-m-albumpanel-facts' }, props.pressing ? h('div', null, props.pressing) : null, props.labels ? h('div', null, props.labels) : null) : null,
        props.unplayable ? h('div', { className: 'bae-m-albumpanel-unplayable' }, props.unplayable) : null,
        h('div', { className: 'bae-m-albumpanel-actions' }, h(MacButton, { kind: 'primary' }, 'Play'), h('span', { className: 'bae-m-albumpanel-more', 'aria-label': 'Album options' }, h(Glyph, { set: 'mac', name: 'ellipsis', size: 13 }))),
        h('div', { className: 'bae-m-albumpanel-tracks' }, props.children)));
  }

  // A side's tracks in two columns once it has more than eight.
  function MacTrackColumns(props) {
    return h('div', { className: 'bae-m-trackcols' }, props.children);
  }

  // Artists and composers: the list on the left, the selected one's page on the right.
  function MacBrowseSplit(props) {
    var list = kids(props.children);
    return h('div', { className: 'bae-m-split' },
      h('div', { className: 'bae-m-split-list' }, list[0]),
      h('div', { className: 'bae-m-split-detail' }, list.slice(1)));
  }

  function MacArtistAlbumTile(props) {
    return h('div', { className: 'bae-m-atile' }, h(Artwork, { title: props.title, radius: 6, missing: props.noCover }),
      h('div', { className: 'bae-m-tile-title' }, props.title), h('div', { className: 'bae-m-atile-year' }, props.year || ' '));
  }


  // A sort criterion: field menu, direction, remove (when there are several); then + to add one.
  function MacSortCriteria(props) {
    var items = String(props.criteria || 'Date Added:down').split(',').map(function (c) { var p = c.trim().split(':'); return { field: p[0], dir: p[1] || 'up' }; });
    var arrow = function (d) { return h('svg', { width: 11, height: 11, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2.4, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: d === 'down' ? 'M12 4v16M5 13l7 7 7-7' : 'M12 20V4M5 11l7-7 7 7' })); };
    return h('div', { className: 'bae-m-sortrow' },
      items.map(function (it, i) {
        return h('span', { key: i, className: 'bae-m-sortpill' },
          h('span', { className: 'bae-m-sortpill-field' }, it.field, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-muted' })),
          h('span', { className: 'bae-muted', 'aria-label': it.dir === 'down' ? 'Descending' : 'Ascending' }, arrow(it.dir)),
          items.length > 1 ? h('span', { className: 'bae-muted', 'aria-label': 'Remove sort criterion' }, h(Glyph, { set: 'mac', name: 'close', size: 8 })) : null);
      }),
      props.full ? null : h('span', { className: 'bae-m-sortadd', 'aria-label': 'Add sort criterion' }, h(Glyph, { set: 'mac', name: 'add', size: 13 })));
  }
  SF.add = 'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z';

  function MacCheckbox(props) {
    return h('label', { className: 'bae-m-checkbox' }, h('span', { className: cx('bae-m-checkbox-box', props.on && 'bae-m-checkbox-on') }, props.on ? h(CheckGlyph) : null), props.label);
  }

  // Names an artist above their albums when the grid is grouped by artist.
  function MacArtistGroupHeading(props) {
    return h('div', { className: 'bae-m-groupheading' }, props.name);
  }

  // A Material menu: `AndroidMenuItem`s, a "Sort by" style label, dividers.
  function AndroidMenu(props) {
    var style = { position: 'absolute', top: Number(props.top || 56), minWidth: Number(props.width || 200) };
    if (props.left != null) style.left = Number(props.left); else style.right = Number(props.right != null ? props.right : 8);
    return h('div', { className: 'bae-a-menu', role: 'menu', style: style }, props.children);
  }
  function AndroidMenuItem(props) {
    if (props.divider) return h('div', { className: 'bae-a-menu-divider', role: 'separator' });
    if (props.heading) return h('div', { className: 'bae-a-menu-heading' }, props.children);
    return h('div', { className: cx('bae-a-menu-item', props.selected && 'bae-a-menu-item-on', props.pressed && 'bae-a-menu-item-pressed'), role: 'menuitem' },
      h('span', null, props.children),
      props.check != null ? h('span', { className: 'bae-a-menu-check', style: { opacity: props.check ? 1 : 0 } }, h(Material, { name: 'check', size: 24 })) : null);
  }
  MATERIAL.check = 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z';

  // ---- Android library ----

  function AndroidPlayPause(props) {
    var icon = Number(props.size || 24), box = icon + 16;
    return h('span', { className: 'bae-a-playpause', style: { width: box, height: box } },
      props.waiting ? h('svg', { className: 'bae-waitarc', width: box, height: box, viewBox: '0 0 48 48', 'aria-label': 'Downloading the track…' }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })) : null,
      h(Glyph, { name: props.playing || props.waiting ? 'pause' : 'play', size: icon }));
  }

  function AndroidNowPlayingBar(props) {
    if (!props.title && !props.error) return null;
    return h('div', { className: 'bae-a-npbar' },
      h('div', { className: 'bae-a-np-row' },
        props.error ? h('div', { className: 'bae-a-np-info bae-a-np-error' }, props.error) : h('div', { className: 'bae-a-np-info' }, h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }),
          h('div', { className: 'bae-a-np-text' }, h('div', { className: 'bae-a-np-title' }, props.title), h('div', { className: 'bae-a-np-sub' }, props.artist))),
        h(AndroidIconButton, { box: 40, icon: 'previous', label: 'Previous' }),
        h(AndroidPlayPause, { playing: props.playing, waiting: props.waiting }),
        h(AndroidIconButton, { box: 40, icon: 'next', label: 'Next' }),
        props.castOff ? null : h(AndroidIconButton, { box: 40, icon: 'cast', label: 'Cast', tone: props.casting ? 'accent' : 'muted' }),
        h(AndroidIconButton, { box: 40, icon: 'queue', label: 'Queue' }),
        h(AndroidIconButton, { box: 40, icon: 'repeat', label: 'Repeat', tone: props.repeat ? 'accent' : 'muted' })),
      h(AndroidSeekBar, { position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration }));
  }

  function AndroidLibraryScreen(props) {
    var collapsed = !!props.collapsed;
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      props.searching ? h('header', { className: 'bae-a-searchbar' }, h(AndroidIconButton, { icon: 'back', label: 'Back' }),
          h('div', { className: 'bae-a-searchfield' }, h('span', { className: props.search ? null : 'bae-a-search-prompt' }, props.search || 'Search albums, tracks, composers, and works'), props.search ? h(Glyph, { name: 'close', size: 20 }) : null))
        : h('header', { className: cx('bae-a-libheader', collapsed && 'bae-a-libheader-collapsed') },
          h('h1', { className: 'bae-a-libhead-title' }, h('span', null, props.view || 'Albums'), h(Glyph, { name: 'down', size: collapsed ? 16 : 20, className: 'bae-a-libhead-chevron' })),
          h(AndroidIconButton, { icon: 'search', label: 'Search' }),
          props.sync === 'moving' ? h('span', { className: 'bae-a-iconbtn' }, h(AndroidSpinner, { size: 16 })) : null,
          props.sync === 'attention' ? h(AndroidIconButton, { icon: 'warning', label: 'Activity', tone: 'warning' }) : null,
          props.sync === 'paused' ? h(AndroidIconButton, { icon: 'pause', label: 'Activity' }) : null,
          h(AndroidIconButton, { icon: 'more', label: 'More options' })),
      h('main', { className: 'bae-a-libbody' }, props.children),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, playing: props.playing, waiting: props.waiting, position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat, error: props.barError }));
  }

  function AndroidDetailScreen(props) {
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      h('header', { className: 'bae-a-detailbar' },
        h(AndroidIconButton, { icon: 'back', label: props.back ? 'Back to ' + props.back : 'Back to Library' }),
        h('span', { style: { flex: 1 } }),
        props.menu ? h(AndroidIconButton, { icon: 'more', label: 'Album options' }) : null),
      h('main', { className: props.flush ? 'bae-a-libbody' : 'bae-a-libbody bae-a-detailbody' }, props.children),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, playing: props.playing, waiting: props.waiting, position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat }));
  }

  function AndroidAlbumGrid(props) {
    return h('div', { className: 'bae-a-grid' }, props.children);
  }

  function AndroidAlbumTile(props) {
    return h('div', { className: 'bae-a-tile' },
      h(Artwork, { title: props.title, missing: props.noCover, radius: 10 }),
      h('div', { className: 'bae-a-tile-title' }, props.title),
      h('div', { className: 'bae-a-tile-line' }, props.line));
  }

  function AndroidArtistHeading(props) {
    if (props.pinned) return h('div', { className: 'bae-a-artisthead-pinned' }, h(Artwork, { title: props.name, size: 32, round: true, missing: props.noImage }), h('div', { className: 'bae-a-artisthead-name' }, props.name));
    return h('div', { className: 'bae-a-artisthead' }, h(Artwork, { title: props.name, size: 48, round: true, missing: props.noImage }),
      h('div', null, h('div', { className: 'bae-a-artisthead-name' }, props.name), h('div', { className: 'bae-a-artisthead-count' }, props.count)));
  }

  function AndroidTrackRow(props) {
    return h('div', { className: 'bae-a-track' },
      h('span', { className: 'bae-a-track-lead' }, props.current ? h(Glyph, { name: props.playing ? 'nowplaying' : 'play', size: 24, className: 'bae-accent' }) : h('span', { className: 'bae-a-track-pos' }, props.position)),
      h('div', { className: 'bae-a-track-text' }, h('div', { className: cx('bae-a-track-title', props.current && 'bae-accent') }, props.title), props.artist ? h('div', { className: 'bae-a-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-a-track-time' }, props.duration),
      h(AndroidIconButton, { icon: 'more', label: 'Track options', tone: 'muted' }));
  }

  function AndroidEyebrow(props) { return h('div', { className: 'bae-a-eyebrow' }, props.children); }

  function AndroidAlbumHeader(props) {
    return h('div', { className: 'bae-a-albumhead' }, h(Artwork, { title: props.title, size: 140, radius: 10, missing: props.noCover }),
      h('div', { className: 'bae-a-albumhead-text' }, h('div', { className: 'bae-a-albumhead-title' }, props.title), h('div', { className: 'bae-a-albumhead-artist' }, props.artist), h('div', { className: 'bae-a-albumhead-fact' }, props.year),
        props.pressing ? h('div', { className: 'bae-a-albumhead-fact bae-a-albumhead-gap' }, props.pressing) : null, props.labels ? h('div', { className: 'bae-a-albumhead-fact' }, props.labels) : null));
  }

  // The album page's pin control for one release, as ReleaseDownloadControl draws it: Pin; Queued to pin, with Cancel; Pinning with its bar and bytes and Cancel pinning; Couldn't pin with Details and Retry, and Cancel; Pinned with Unpin. On a release this phone can't play, one line saying where it is. Nothing says how many wait ahead or what holds the queue back.
  function AndroidDownloadControl(props) {
    var s = props.state || 'available';
    function btn(icon, text) { return h('button', { type: 'button', className: 'bae-a-btn bae-a-btn-outlined bae-a-btn-icon' }, h(Glyph, { name: icon, size: 20 }), text); }
    var line = function (t) { return h('span', { className: 'bae-a-fineline' }, t); };
    var body =
      s === 'elsewhere' ? line(props.where || 'On Ana’s Mac') :
      s === 'queued' ? h('div', { className: 'bae-a-dlrow' }, line('Queued to pin'), btn('close', 'Cancel')) :
      s === 'downloading' ? [h(AndroidProgress, { key: 'p', value: Number(props.progress || 0.4) }), h('span', { key: 'b', className: 'bae-a-fine' }, 'Pinning · ' + (props.bytes || '120 MB of 300 MB')), h('div', { key: 'c' }, btn('close', 'Cancel pinning'))] :
      s === 'failed' ? [h(AndroidNotice, { key: 'e', quiet: true, tone: 'danger', title: 'Couldn’t pin', secondary: 'Details', action: 'Retry' }), h('div', { key: 'r' }, btn('close', 'Cancel'))] :
      s === 'downloaded' ? h('div', { className: 'bae-a-dlrow' }, h(Glyph, { name: 'pin', size: 16, className: 'bae-muted' }), line('Pinned'), btn('unpin', 'Unpin')) :
      btn('pin', 'Pin');
    return h('div', { className: 'bae-a-dl' }, body);
  }

  // The album page's upload line, as ReleaseUploadLine draws it: the phase core words, Queued to upload while the files wait; a missing original (`missing`) as a quiet warning notice naming it.
  function AndroidUploadLine(props) {
    return h('div', { className: 'bae-a-dl' }, h('span', { className: 'bae-a-fineline' }, props.phase || 'Queued to upload'),
      props.missing ? h(AndroidNotice, { quiet: true, tone: 'warning', title: props.missing + ' is missing', subtitle: 'Restore it to finish uploading.' }) : null);
  }
