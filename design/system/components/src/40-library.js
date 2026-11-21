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
    return h('div', { className: cx('bae-art', props.missing && 'bae-art-missing', props.shadow && 'bae-art-shadow-' + props.shadow, props.ring && 'bae-art-ring', props.className), style: style, role: props.label ? 'img' : null, 'aria-label': props.label || null, 'aria-hidden': props.label ? null : true }, props.children);
  }

  // A person's picture, round: their image when they have one, or the initials of their name on a tint. `image` stands for the picked image (the board draws it as artwork seeded by it).
  function Avatar(props) {
    var s = Number(props.size || 32), name = String(props.name || '?');
    var ini = name.split(/\s+/).map(function (p) { return p[0]; }).join('').slice(0, 2).toUpperCase();
    return props.image ? h(Artwork, { title: props.image, size: s, round: true }) : h('span', { className: 'bae-avatar', 'aria-hidden': true, style: { width: s, height: s, fontSize: s * 0.4 } }, ini);
  }

  // A playlist's picture: the covers of its first four different releases in a 2×2 square, as playlists show on both platforms; with fewer than four, its first entry's cover; empty, the missing tile. `covers` lists the releases' titles in the playlist's order, each once. One cover alone would make a playlist look like that album, and two playlists that start on one album alike.
  function PlaylistPicture(props) {
    var covers = String(props.covers || '').split(',').map(function (x) { return x.trim(); }).filter(Boolean);
    var size = Number(props.size || 48), radius = props.radius != null ? Number(props.radius) : 6;
    if (covers.length < 4) return h(Artwork, { title: covers[0], size: size, radius: radius, missing: !covers.length });
    return h('div', { className: 'bae-playlist-pic', style: { width: size, height: size, borderRadius: radius }, 'aria-hidden': true },
      covers.slice(0, 4).map(function (t, i) { return h(Artwork, { key: i, title: t, size: size / 2, radius: 0 }); }));
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
    warning: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20zM12 3.6a8.4 8.4 0 1 1 0 16.8 8.4 8.4 0 1 1 0-16.8zM10.9 6.5h2.2l-.3 8h-1.6zM12 15.8a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 1 0 0-2.6z',
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
  // A time as m:ss or h:mm:ss, in seconds.
  function seconds(t) { return String(t || '0').split(':').reduce(function (n, p) { return n * 60 + Number(p); }, 0); }
  // The line fills the elapsed time's share of the duration.
  function SeekLine(prefix) {
    return function (props) {
      var total = seconds(props.duration), pos = total ? seconds(props.elapsed) / total : 0;
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
    return h('button', { type: 'button', disabled: props.off ? true : null, className: cx('bae-m-iconbtn', props.active && 'bae-m-iconbtn-on', props.faint && 'bae-m-iconbtn-faint', props.small && 'bae-m-iconbtn-small', props.boxed && 'bae-m-iconbtn-boxed', props.overlay && 'bae-m-iconbtn-overlay', props.off && 'bae-off'), 'aria-label': props.label || props.icon, 'aria-haspopup': props.menu ? 'menu' : null, 'aria-expanded': props.menu ? !!props.active : null, title: props.help || props.label },
      h(Glyph, { set: 'mac', name: props.icon, size: props.size || 17 }));
  }

  // A row's ⋯ button with the row's actions in its menu: a row offers its actions here, not as a button of each. `open` draws the menu under it, `items` its entries ("-" for a line between groups), `highlighted` the one under the pointer; `off` dims it while none of its actions can run. `boxed` fills it as a push button, for a page's header; `overlay` makes it a round button on a dark fill, for a place on a cover.
  function MacMoreMenu(props) {
    var btn = h(MacIconButton, { icon: 'ellipsis', size: 15, label: props.label || 'More', active: props.open, menu: true, off: props.off, boxed: props.boxed, overlay: props.overlay });
    if (!props.open) return btn;
    return h('span', { className: 'bae-m-moremenu' }, btn, h('div', { className: 'bae-m-moremenu-pop' }, h(MacMenu, null, list(props.items).map(function (t, i) {
      return t === '-' ? h(MacMenuItem, { key: i, divider: true }) : h(MacMenuItem, { key: i, highlighted: t === props.highlighted }, t);
    }))));
  }
  // The same on Android: the row's ⋮, its menu under it when `open`; `off` dims it while none of its actions can run.
  function AndroidMoreMenu(props) {
    var btn = h(AndroidIconButton, { icon: 'more', label: 'More', off: props.off, menu: true });
    if (!props.open) return btn;
    return h('span', { className: 'bae-a-moremenu' }, btn, h(AndroidMenu, { top: 44, right: 0 }, list(props.items).map(function (t, i) {
      return t === '-' ? h(AndroidMenuItem, { key: i, divider: true }) : h(AndroidMenuItem, { key: i, pressed: t === props.highlighted }, t);
    })));
  }

  function MacPlayPause(props) {
    // A button named by what it does, as the app's: the arc around it while the track waits on storage is drawing only, with no words of its own.
    return h('button', { type: 'button', className: 'bae-m-playpause', 'aria-label': props.playing ? 'Pause' : 'Play' },
      props.waiting ? h('svg', { className: 'bae-waitarc', width: 48, height: 48, viewBox: '0 0 48 48', 'aria-hidden': true }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })) : null,
      h(Glyph, { set: 'mac', name: props.playing ? 'pause' : 'play', size: 20 }));
  }

  // Shuffle, previous, play and pause, next, repeat; `shuffle` and `repeat` light theirs while on, `waiting` draws the arc around play while the track waits on storage.
  function MacTransport(props) {
    return h('div', { className: 'bae-m-transport' },
      h(MacIconButton, { icon: 'shuffle', label: props.shuffle ? 'Turn off shuffle' : 'Shuffle', active: props.shuffle }),
      h(MacIconButton, { icon: 'previous', label: 'Previous' }),
      h(MacPlayPause, { playing: props.playing, waiting: props.waiting }),
      h(MacIconButton, { icon: 'next', label: 'Next' }),
      h(MacIconButton, { icon: 'repeat', label: 'Repeat', active: props.repeat }));
  }

  // Mute and the volume slider, filled to `volume` (0 to 1). `noVolume`: playing on a receiver that takes no volume from bae (a UPnP renderer with no volume control): no Mute and no slider, and a line naming where the volume is set.
  function MacVolumeSlider(props) {
    if (props.noVolume) return h('span', { className: 'bae-m-np-novolume' }, 'Volume is set on ' + props.noVolume);
    return h('div', { className: 'bae-m-volumeslider' },
      h(MacIconButton, { icon: 'speaker', label: 'Mute' }),
      h('div', { className: 'bae-m-volume' }, h('div', { style: { width: ((props.volume != null ? Number(props.volume) : 0.7) * 100) + '%' } })));
  }

  function MacNowPlayingBar(props) {
    var none = !props.title;
    return h('div', { className: 'bae-m-npbar' },
      // The playing track: its cover beside its title and artist.
      h('div', { className: 'bae-m-np-left' }, none ? null : h('div', { className: 'bae-m-nptrack' },
        h(Artwork, { title: props.album || props.title, size: 56, radius: 6, shadow: 'bar', missing: props.noCover }),
        h('div', { className: 'bae-m-np-text' }, h('div', { className: 'bae-m-np-title' }, props.title), h('div', { className: 'bae-m-np-sub' }, props.artist)))),
      h('div', { className: 'bae-m-np-center' },
        h(MacTransport, { playing: props.playing, waiting: props.waiting, shuffle: props.shuffle, repeat: props.repeat }),
        h(MacSeekBar, { held: props.held, elapsed: props.elapsed, duration: props.duration, off: none })),
      h('div', { className: 'bae-m-np-right' },
        h(MacIconButton, { icon: 'cast', label: 'Play on…', help: props.casting ? 'Playing on ' + props.casting : null, active: !!props.casting }),
        h(MacIconButton, { icon: 'queue', label: 'Queue', active: props.queueOpen }),
        h(MacVolumeSlider, { volume: props.volume, noVolume: props.noVolume })));
  }

  // Library and Import, the main window's two sections; `section` is the one showing.
  function MacSectionTabs(props) {
    var section = props.section || 'Library';
    return h('div', { className: 'bae-m-sections', role: 'tablist' }, ['Library', 'Import'].map(function (s) {
      return h('span', { key: s, role: 'tab', 'aria-selected': s === section, className: cx('bae-m-section-tab', s === section && 'bae-m-section-tab-on') }, s);
    }));
  }

  // The title bar's Activity button, by what the sync queue does (`state`): moving spins, attention shows the warning mark, and paused (a queue held, Paused or Waiting for network, with nothing moving) shows a pause symbol so the hold can be seen. With nothing moving or held there is no button.
  function MacActivityButton(props) {
    if (props.state === 'moving') return h('span', { className: 'bae-m-iconbtn', title: 'Activity' }, h(MacSpinner, { size: 14 }));
    if (props.state === 'attention') return h('span', { className: 'bae-m-iconbtn bae-m-sync-warn', title: 'Activity' }, h(Glyph, { set: 'mac', name: 'warning', size: 17 }));
    if (props.state === 'paused') return h(MacIconButton, { icon: 'pause', label: 'Activity' });
    return null;
  }

  // The person's picture, or initials, that opens their menu: Add device…, Invite someone… (or Set up a cloud home…), Playlists, History, Settings…. `name`: their name (the boards' person is Ana); `image`: their picture.
  function MacAvatarButton(props) {
    return h('button', { type: 'button', 'aria-haspopup': 'menu', 'aria-label': 'Your menu', className: 'bae-m-iconbtn bae-m-avatarbtn' }, h(Avatar, { name: props.name || 'Ana', image: props.image, size: 22 }));
  }

  // The main window's title bar: the section tabs in the middle, then the search field, the Activity button (`sync`) and the avatar at the end.
  function MacTitleBar(props) {
    return h('header', { className: 'bae-m-titlebar' },
      h('div', { className: 'bae-m-titlebar-center' }, h(MacSectionTabs, { section: props.section })),
      h('div', { className: 'bae-m-titlebar-trail' },
        h(MacSearchField, { value: props.search, focused: props.searchFocused }),
        h(MacActivityButton, { state: props.sync }),
        h(MacAvatarButton, { name: props.avatar, image: props.avatarImage })));
  }

  // The main window's frame, its own as in the app (MainWindowChrome): the rounded window with the traffic lights, the title bar over the library or Import and the side pane, then the now playing bar.
  function MacMainWindow(props) {
    return h('div', { className: 'bae-m-mainwin', style: { width: Number(props.width || 1200), height: Number(props.height || 760) } }, macLights(),
      h(MacTitleBar, { section: props.section, search: props.search, searchFocused: props.searchFocused, sync: props.sync, avatar: props.avatar, avatarImage: props.avatarImage }),
      h('div', { className: 'bae-m-main-body' }, h('div', { className: 'bae-m-main-content' }, kids(props.children).filter(function (c) { var p = inner(c); return !(p && p.type === MacQueuePanel); })), kids(props.children).filter(function (c) { var p = inner(c); return p && p.type === MacQueuePanel; })),
      h(MacNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, noCover: props.trackNoCover, playing: props.playing, waiting: props.waiting, held: props.held, elapsed: props.elapsed, duration: props.duration, shuffle: props.shuffle, repeat: props.repeat, casting: props.casting, noVolume: props.noVolume, queueOpen: props.queueOpen }));
  }

  function MacLibraryHeading(props) {
    var collapsed = !!props.collapsed;
    return h('div', { className: cx('bae-m-libhead', collapsed && 'bae-m-libhead-collapsed') },
      h('h1', { className: 'bae-m-libhead-title' }, props.view || 'Albums', h('span', { className: 'bae-m-libhead-chevron', 'aria-hidden': true }, h(Glyph, { set: 'mac', name: 'down', size: collapsed ? 8 : 15 }))),
      h('div', { className: 'bae-m-libhead-trail' }, props.children));
  }
  SF.down = 'M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6 1.41-1.41z';
  SF.sidebar = 'M3 5h18v14H3V5zm2 2v10h9V7H5zm11 0v10h3V7h-3z';

  function MacAlbumGrid(props) {
    var n = Number(props.columns || 4);
    return h('div', { className: 'bae-m-grid', style: { gridTemplateColumns: 'repeat(' + n + ', minmax(0, 1fr))' } }, props.children);
  }

  function MacAlbumTile(props) {
    return h('div', { className: cx('bae-m-tile', props.selected && 'bae-m-tile-selected') },
      h('div', { className: 'bae-m-tile-art' },
        h(Artwork, { title: props.title, missing: props.noCover, radius: 10, shadow: 'tile', ring: props.open }),
        props.hover ? h('span', { className: 'bae-m-tile-menu' }, h(MacMoreMenu, { overlay: true, label: 'Album options' })) : null),
      h('div', { className: 'bae-m-tile-title' }, props.title),
      h('div', { className: 'bae-m-tile-artist' }, props.artist),
      h('div', { className: 'bae-m-tile-year' }, props.year || ' '));
  }

  // A track row's lead: the track's number; under the pointer (`hover`) Play, or Pause on the current track while it plays; on the current track otherwise the speaker.
  function MacTrackLead(props) {
    return h('span', { className: 'bae-m-track-lead' },
      props.hover ? h(Glyph, { set: 'mac', name: props.current && props.playing ? 'pause' : 'play', size: 13 })
        : props.current ? h(Glyph, { set: 'mac', name: 'speaker', size: 11, className: 'bae-accent' })
        : h('span', { className: 'bae-m-track-pos' }, props.position));
  }

  // One track of an album's list. `picked`: among the tracks picked together, on the soft accent. `rating`: the row of an album's list, which keeps a slot for the track's stars after the row (0 leaves the slot empty, as the app does for a track nobody rated).
  function MacTrackRow(props) {
    var row = h('div', { className: cx('bae-m-track', props.picked ? 'bae-m-track-picked' : props.hover && 'bae-m-track-hover', props.artist && 'bae-m-track-two') },
      h(MacTrackLead, { position: props.position, hover: props.hover, current: props.current, playing: props.playing }),
      h('div', { className: 'bae-m-track-text' },
        h('div', { className: cx('bae-m-track-title', props.current && 'bae-accent') }, props.title),
        props.artist ? h('div', { className: 'bae-m-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-m-track-time' }, props.duration));
    // Every album list keeps a slot for the track's stars after the row, empty when you gave it none, so every row's duration lines up.
    var n = Number(props.rating || 0);
    return h('div', { className: 'bae-m-track-rated' }, h('div', { className: 'bae-m-track-rated-row' }, row),
      h('span', { className: cx('bae-m-track-stars', !n && 'bae-m-track-stars-none'), 'aria-hidden': !n || null }, h(Stars, { value: n, size: 10 })));
  }

  function MacEyebrow(props) { return h('div', { className: 'bae-m-eyebrow' }, props.children); }

  // `options`: the page's ⋯ button at the right with these actions in its menu, as an album's options button; `optionsOpen` draws the menu open, `highlighted` the item under the pointer.
  function MacDetailHeader(props) {
    return h('div', { className: 'bae-m-dethead' },
      h(Artwork, { title: props.name, size: 72, radius: 10, missing: props.noImage }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-m-dethead-name' }, props.name), h('div', { className: 'bae-m-dethead-count' }, props.count)),
      props.options ? h(MacMoreMenu, { label: props.optionsLabel || 'Options', items: props.options, open: props.optionsOpen, highlighted: props.highlighted }) : null);
  }

  // A playlist page's head, laid out as the album page's: the picture beside a column of the name (up to two lines), the owner on a line of their own, the facts line (count and length), then Play and the ⋯ under the text, at its start. The ⋯ menu offers Shuffle to everyone, then the owner's `options`; `optionsOpen` draws it open, `highlighted` the item under the pointer. The picture is as tall as the text column. `sharing`: on the person's own playlist, who sees it, as a lock (Only me) or people (Everyone) glyph at the facts line's end; a playlist someone else shared shows nothing about sharing.
  function MacPlaylistHeader(props) {
    var shared = props.sharing ? h('span', { className: 'bae-m-plpage-sharing', role: 'img', 'aria-label': props.sharing, title: props.sharing }, h(Glyph, { set: 'mac', name: props.sharing === 'Everyone' ? 'people' : 'lock', size: 12 })) : null;
    return h('div', { className: 'bae-m-plpage-head' },
      h(PlaylistPicture, { covers: props.covers, size: 103, radius: 10 }),
      h('div', { className: 'bae-m-plpage-text' },
        h('h1', { className: 'bae-m-plpage-name' }, props.name),
        h(MacPersonTag, { name: props.owner, strong: true }),
        h('div', { className: 'bae-m-plpage-facts' }, [props.count, props.length].join(' · '), shared),
        h('div', { className: 'bae-m-plpage-actions' },
          h(MacButton, { kind: 'primary' }, 'Play'),
          h(MacMoreMenu, { boxed: true, label: 'Playlist options', items: ['Shuffle'].concat(list(props.options)).join(','), open: props.optionsOpen, highlighted: props.highlighted }))));
  }

  // A playlist's page inside the side pane's Playlists tab, whose head shows the back button to the list (`MacQueuePanel`'s `back`): its header (`MacPlaylistHeader`, which takes the same props), then its tracks as children, `MacListRow`s.
  function MacPlaylistPage(props) {
    return h('section', { className: 'bae-m-plpage' },
      h(MacPlaylistHeader, { name: props.name, owner: props.owner, covers: props.covers, count: props.count, length: props.length, sharing: props.sharing, options: props.options, optionsOpen: props.optionsOpen, highlighted: props.highlighted }),
      h('div', { className: 'bae-m-plpage-tracks' }, props.children));
  }

  // A queued track: the playlist page's row, the cover beside the title over the artists, the length at the end; under the pointer (`hover`) the row tints and Remove from queue joins the end.
  function MacQueueRow(props) {
    return h(MacListRow, { cover: 40, title: props.title, sub: props.artist, album: props.album, duration: props.duration, hover: props.hover, remove: 'Remove from queue' });
  }


  // The close control of a docked pane and of an open album: an × in a round, quiet fill.
  function MacCloseButton(props) {
    return h('button', { type: 'button', className: 'bae-m-close', 'aria-label': props.label || 'Close', title: props.label || 'Close' }, h(Glyph, { set: 'mac', name: 'close', size: 13 }));
  }

  // The choice of an album's release, one segment for each of `releases` (comma-separated), `selected` the one showing; an album of one release has no choice to draw.
  function MacReleasePicker(props) {
    var releases = list(props.releases);
    if (releases.length < 2) return null;
    return h(MacSegmentedControl, { options: releases.join(','), selected: props.selected || releases[0] });
  }

  // An album's stars (`value`, 0 for none yet) and, beside them, `who`: a line of who else rated it.
  function MacAlbumRating(props) {
    return h('div', { className: 'bae-m-albumrating' }, h(Stars, { value: props.value, size: 16, rate: true }), props.who ? h('span', { className: 'bae-m-albumrating-who' }, props.who) : null);
  }

  // The open album's head: the title, the artist and year, the release picker (`releases`, `release`), the facts (`pressing`, `labels`), the line saying why this Mac can't play it (`unplayable`), Play and the album's ⋯ (`options`, `optionsOpen`, `highlighted`, a boxed MacMoreMenu), then the rating row, always: the stars (`rating`, empty while no one has rated it) and who else rated it (`ratingWho`).
  function MacAlbumHeader(props) {
    var picker = h(MacReleasePicker, { releases: props.releases, selected: props.release });
    return h('div', { className: 'bae-m-albumhead' },
      h('h2', { className: 'bae-m-albumpanel-title' }, props.title),
      h('div', { className: 'bae-m-albumpanel-sub' }, h('span', { className: 'bae-muted' }, props.artist), props.year ? [h('span', { key: 'd', className: 'bae-m-tertiary' }, '·'), h('span', { key: 'y', className: 'bae-m-tertiary' }, props.year)] : null),
      list(props.releases).length > 1 ? h('div', { className: 'bae-m-albumpanel-releases' }, picker) : null,
      props.pressing || props.labels ? h('div', { className: 'bae-m-albumpanel-facts' }, props.pressing ? h('div', null, props.pressing) : null, props.labels ? h('div', null, props.labels) : null) : null,
      props.unplayable ? h('div', { className: 'bae-m-albumpanel-unplayable' }, props.unplayable) : null,
      h('div', { className: 'bae-m-albumpanel-actions' }, h(MacButton, { kind: 'primary' }, 'Play'), h(MacMoreMenu, { boxed: true, label: 'Album options', items: props.options, open: props.optionsOpen, highlighted: props.highlighted })),
      h('div', { className: 'bae-m-albumhead-rating' }, h(MacAlbumRating, { value: props.rating || 0, who: props.ratingWho })));
  }

  // An album's credits: the Credits heading over its role and name pairs (`credits`: "Role: name" pairs separated by semicolons, as `MacFactsGrid` takes them).
  function MacAlbumCredits(props) {
    return h('div', { className: 'bae-m-albumcredits' }, h(MacFormHeading, null, 'Credits'), h(MacFactsGrid, { facts: props.credits, wide: true }));
  }

  // The works an album's tracks record: the Works heading over one line a work, its track's number, the work's title in the accent and its composer (`works`: "number|title|composer" entries separated by semicolons).
  function MacAlbumWorks(props) {
    var works = String(props.works || '').split(';').map(function (w) { return w.split('|').map(function (x) { return x.trim(); }); }).filter(function (w) { return w[1]; });
    return h('div', { className: 'bae-m-albumcredits' }, h(MacFormHeading, null, 'Works'),
      h('div', { className: 'bae-m-albumworks' }, works.map(function (w, i) {
        return h('div', { key: i, className: 'bae-m-albumworks-row' }, h('span', { className: 'bae-muted bae-num bae-m-albumworks-pos' }, w[0]), h('span', { className: 'bae-accent' }, w[1]), w[2] ? h('span', { className: 'bae-muted' }, '· ' + w[2]) : null);
      })));
  }

  // The album opened in the grid: a panel under the album's row, its 340pt cover beside its header (`MacAlbumHeader`, which takes the same props) and then its tracks as children.
  function MacAlbumPanel(props) {
    return h('section', { className: 'bae-m-albumpanel' },
      h('span', { className: 'bae-m-albumpanel-close' }, h(MacCloseButton)),
      h(Artwork, { title: props.title, size: 340, radius: 10, missing: props.noCover, className: 'bae-m-albumpanel-cover' }),
      h('div', { className: 'bae-m-albumpanel-body' },
        h(MacAlbumHeader, { title: props.title, artist: props.artist, year: props.year, releases: props.releases, release: props.release, pressing: props.pressing, labels: props.labels, unplayable: props.unplayable, options: props.options, optionsOpen: props.optionsOpen, highlighted: props.highlighted, rating: props.rating, ratingWho: props.ratingWho }),
        h('div', { className: 'bae-m-albumpanel-tracks bae-m-albumpanel-tracks-rated' }, props.children)));
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


  // An artist page's albums, `MacArtistAlbumTile`s as children, as many to a row as fit.
  function MacArtistAlbumGrid(props) {
    return h('div', { className: 'bae-m-artistgrid' }, props.children);
  }

  // An artist page's other names: the Also credited as label over the field that adds one, with Add, which stays off until the field holds a name (`value`).
  function MacArtistOtherNames(props) {
    return h('div', { className: 'bae-m-othernames' },
      h('div', { className: 'bae-m-othernames-label' }, 'Also credited as'),
      h('div', { className: 'bae-m-othernames-add' }, h(MacTextField, { placeholder: 'Another name', value: props.value }), h(MacButton, { off: !props.value }, 'Add')));
  }

  // One sort criterion: its field's menu, its direction (`direction`: up or down) and, when the sort has several criteria (`removable`), remove.
  function MacSortPill(props) {
    var down = props.direction === 'down';
    var arrow = h('svg', { width: 11, height: 11, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2.4, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: down ? 'M12 4v16M5 13l7 7 7-7' : 'M12 20V4M5 11l7-7 7 7' }));
    return h('span', { className: 'bae-m-sortpill' },
      h('span', { className: 'bae-m-sortpill-field' }, props.field, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-muted' })),
      h('span', { className: 'bae-muted', 'aria-label': down ? 'Descending' : 'Ascending' }, arrow),
      props.removable ? h('span', { className: 'bae-muted', 'aria-label': 'Remove sort criterion' }, h(Glyph, { set: 'mac', name: 'close', size: 8 })) : null);
  }

  // The sort's criteria (`criteria`: "Field:down,Field:up"), each a `MacSortPill`; then + to add one, while a field of the list (`fields`, the albums' Title, Artist, Year and Date added unless given) is still unused.
  function MacSortCriteria(props) {
    var items = String(props.criteria || 'Date added:down').split(',').map(function (c) { var p = c.trim().split(':'); return { field: p[0], dir: p[1] || 'up' }; });
    return h('div', { className: 'bae-m-sortrow' },
      items.map(function (it, i) { return h(MacSortPill, { key: i, field: it.field, direction: it.dir, removable: items.length > 1 }); }),
      list(props.fields || 'Title, Artist, Year, Date added').every(function (f) { return items.some(function (it) { return it.field === f; }); }) ? null : h('span', { className: 'bae-m-sortadd', 'aria-label': 'Add sort criterion' }, h(Glyph, { set: 'mac', name: 'add', size: 13 })));
  }
  SF.add = 'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z';
  // Who sees a playlist of the person's own: a lock for Only me, two people for Everyone.
  SF.lock = 'M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1 1.71 0 3.1 1.39 3.1 3.1v2z';
  SF.people = 'M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5c-1.66 0-3 1.34-3 3s1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5C6.34 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z';
  // The filled circle with an × that removes one entry of a list (xmark.circle.fill).
  SF.removefill = 'M12 2C6.47 2 2 6.47 2 12s4.47 10 10 10 10-4.47 10-10S17.53 2 12 2zm5 13.59L15.59 17 12 13.41 8.41 17 7 15.59 10.59 12 7 8.41 8.41 7 12 10.59 15.59 7 17 8.41 13.41 12 17 15.59z';

  function MacCheckbox(props) {
    return h('label', { className: 'bae-m-checkbox', role: 'checkbox', 'aria-checked': !!props.on }, h('span', { className: cx('bae-m-checkbox-box', props.on && 'bae-m-checkbox-on') }, props.on ? h(CheckGlyph) : null), props.label);
  }

  // Names an artist above their albums when the grid is grouped by artist.
  function MacArtistGroupHeading(props) {
    return h('div', { className: 'bae-m-groupheading' }, props.name);
  }

  // A Material menu: `AndroidMenuItem`s, a "Sort by" style label, dividers.
  function AndroidMenu(props) {
    // A menu sizes to its items; `width` is a minimum only a board that needs one passes.
    var style = { position: 'absolute', top: Number(props.top || 56) };
    if (props.width != null) style.minWidth = Number(props.width);
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
    return h('button', { type: 'button', className: 'bae-a-playpause', style: { width: box, height: box }, 'aria-label': props.playing || props.waiting ? 'Pause' : 'Play' },
      props.waiting ? h('svg', { className: 'bae-waitarc', width: box, height: box, viewBox: '0 0 48 48', 'aria-hidden': true }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })) : null,
      h(Glyph, { name: props.playing || props.waiting ? 'pause' : 'play', size: icon }));
  }

  // The playing track on Android: its cover beside its title and artist.
  function AndroidNowPlayingTrack(props) {
    // One button that opens the full player, named as TalkBack reads the merged track.
    return h('button', { type: 'button', className: 'bae-a-np-info', 'aria-label': props.artist ? 'Now playing, ' + props.title + ' by ' + props.artist : props.title }, h(Artwork, { title: props.album || props.title, size: 48, radius: 6, missing: props.noCover }),
      h('div', { className: 'bae-a-np-text', 'aria-hidden': true }, h('div', { className: 'bae-a-np-title' }, props.title), h('div', { className: 'bae-a-np-sub' }, props.artist)));
  }

  // Previous, play and pause, next. `large` is the full player's: bigger buttons, spread across the sheet; `waiting` draws the arc around play while the track waits on storage.
  function AndroidTransport(props) {
    var large = !!props.large;
    return h('div', { className: cx('bae-a-transport', large && 'bae-a-transport-large') },
      h(AndroidIconButton, { box: large ? 56 : 40, size: large ? 36 : null, icon: 'previous', label: 'Previous' }),
      h(AndroidPlayPause, { playing: props.playing, waiting: props.waiting, size: large ? 48 : null }),
      h(AndroidIconButton, { box: large ? 56 : 40, size: large ? 36 : null, icon: 'next', label: 'Next' }));
  }

  function AndroidNowPlayingBar(props) {
    if (!props.title && !props.error) return null;
    return h('div', { className: 'bae-a-npbar' },
      h('div', { className: 'bae-a-np-row' },
        props.error ? h('div', { className: 'bae-a-np-info bae-a-np-error' }, props.error) : h(AndroidNowPlayingTrack, { title: props.title, artist: props.artist, album: props.album, noCover: props.noCover }),
        h(AndroidTransport, { playing: props.playing, waiting: props.waiting }),
        props.castOff ? null : h(AndroidIconButton, { box: 40, icon: 'cast', label: 'Play on…', tone: props.casting ? 'accent' : 'muted' }),
        h(AndroidIconButton, { box: 40, icon: 'queue', label: 'Queue' }),
        h(AndroidIconButton, { box: 40, icon: 'repeat', label: 'Repeat', tone: props.repeat ? 'accent' : 'muted' })),
      h(AndroidSeekBar, { held: props.held, elapsed: props.elapsed, duration: props.duration }));
  }

  // The library header while searching: back, and the search field with what was typed (`search`) and its clear button, or the prompt.
  function AndroidSearchBar(props) {
    return h('header', { className: 'bae-a-searchbar' }, h(AndroidIconButton, { icon: 'back', label: 'Back' }),
      h('div', { className: 'bae-a-searchfield' }, h('span', { className: props.search ? null : 'bae-a-search-prompt' }, props.search || 'Search albums, tracks, composers, and works'), props.search ? h(Glyph, { name: 'close', size: 20 }) : null));
  }

  // The Activity button, by what the sync queue does (`state`): moving spins, attention shows the warning mark, and paused (a queue held with nothing moving) shows a pause symbol so the hold can be seen. With nothing moving or held there is no button.
  function AndroidActivityButton(props) {
    if (props.state === 'moving') return h('span', { className: 'bae-a-iconbtn' }, h(AndroidSpinner, { size: 16 }));
    if (props.state === 'attention') return h(AndroidIconButton, { icon: 'warning', label: 'Activity', tone: 'warning' });
    if (props.state === 'paused') return h(AndroidIconButton, { icon: 'pause', label: 'Activity' });
    return null;
  }

  // The library's header: the view's title, which opens the view menu, then search, the Activity button (`sync`), ⋮ and the avatar; `collapsed` once the grid scrolls; `searching` swaps it for `AndroidSearchBar`.
  function AndroidLibraryHeader(props) {
    var collapsed = !!props.collapsed;
    if (props.searching) return h(AndroidSearchBar, { search: props.search });
    return h('header', { className: cx('bae-a-libheader', collapsed && 'bae-a-libheader-collapsed') },
      h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-a-libhead-title' }, h('span', null, props.view || 'Albums'), h(Glyph, { name: 'down', size: collapsed ? 16 : 20, className: 'bae-a-libhead-chevron' })),
      h(AndroidIconButton, { icon: 'search', label: 'Search' }),
      h(AndroidActivityButton, { state: props.sync }),
      h(AndroidIconButton, { icon: 'more', label: 'More options' }),
      // The person's picture, or initials, ends the bar and opens their menu: Add device, Invite someone, Playlists, History, Settings. `avatar`: their name (the boards' person is Ana); `avatarImage`: their picture.
      h('button', { type: 'button', 'aria-haspopup': 'menu', 'aria-label': 'Your menu', className: 'bae-a-iconbtn' }, h(Avatar, { name: props.avatar || 'Ana', image: props.avatarImage, size: 32 })));
  }

  function AndroidLibraryScreen(props) {
    var split = splitOverlays(props.children);
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      h(AndroidLibraryHeader, { view: props.view, collapsed: props.collapsed, searching: props.searching, search: props.search, sync: props.sync, avatar: props.avatar, avatarImage: props.avatarImage }),
      h('main', { className: 'bae-a-libbody' }, split.content),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, noCover: props.trackNoCover, playing: props.playing, waiting: props.waiting, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat, error: props.barError }),
      split.over);
  }

  function AndroidDetailScreen(props) {
    var split = splitOverlays(props.children);
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      h(AndroidTopBar, { back: true, backLabel: props.back ? 'Back to ' + props.back : 'Back to library', more: props.menu ? 'Album options' : null }),
      h('main', { className: props.flush ? 'bae-a-libbody' : 'bae-a-libbody bae-a-detailbody' }, split.content),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, noCover: props.trackNoCover, playing: props.playing, waiting: props.waiting, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat }),
      split.over);
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

  function AndroidArtistGroupHeading(props) {
    if (props.pinned) return h('div', { className: 'bae-a-artisthead-pinned' }, h(Artwork, { title: props.name, size: 32, round: true, missing: props.noImage }), h('div', { className: 'bae-a-artisthead-name' }, props.name));
    return h('div', { className: 'bae-a-artisthead' }, h(Artwork, { title: props.name, size: 48, round: true, missing: props.noImage }),
      h('div', null, h('div', { className: 'bae-a-artisthead-name' }, props.name), h('div', { className: 'bae-a-artisthead-count' }, props.count)));
  }

  // A track row's lead on Android: the track's number, or on the current track Play, or the playing mark while it plays.
  function AndroidTrackLead(props) {
    return h('span', { className: 'bae-a-track-lead' }, props.current ? h(Glyph, { name: props.playing ? 'nowplaying' : 'play', size: 24, className: 'bae-accent' }) : h('span', { className: 'bae-a-track-pos' }, props.position));
  }

  // `work`: the work the track records, with its composer, on a line under the row.
  function AndroidTrackRow(props) {
    var row = h('div', { className: 'bae-a-track' },
      h(AndroidTrackLead, { position: props.position, current: props.current, playing: props.playing }),
      h('div', { className: 'bae-a-track-text' }, h('div', { className: cx('bae-a-track-title', props.current && 'bae-accent') }, props.title), props.artist ? h('div', { className: 'bae-a-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-a-track-time' }, props.duration),
      h(AndroidIconButton, { icon: 'more', label: 'Track options', tone: 'muted' }));
    return props.work ? h('div', null, row, h('div', { className: 'bae-a-track-work' }, props.work)) : row;
  }

  function AndroidEyebrow(props) { return h('div', { className: 'bae-a-eyebrow' }, props.children); }

  function AndroidAlbumHeader(props) {
    return h('div', { className: 'bae-a-albumhead' }, h(Artwork, { title: props.title, size: 140, radius: 10, missing: props.noCover }),
      h('div', { className: 'bae-a-albumhead-text' }, h('div', { className: 'bae-a-albumhead-title' }, props.title), h('div', { className: 'bae-a-albumhead-artist' }, props.artist), h('div', { className: 'bae-a-albumhead-fact' }, props.year),
        props.pressing ? h('div', { className: 'bae-a-albumhead-fact bae-a-albumhead-gap' }, props.pressing) : null, props.labels ? h('div', { className: 'bae-a-albumhead-fact' }, props.labels) : null));
  }

  // The album page's pin control for one release, as AndroidDownloadControl draws it: Pin; Queued to pin, with Cancel; Pinning with its bar and bytes and Cancel pinning; Couldn’t pin with Details and Retry, and Cancel; Pinned with Unpin. On a release this phone can't play, one line saying where it is. Nothing says how many wait ahead or what holds the queue back.
  function AndroidPinControl(props) {
    var s = props.state || 'available';
    function btn(icon, text) { return h(AndroidButton, { kind: 'outlined', icon: icon }, text); }
    var line = function (t) { return h('span', { className: 'bae-a-fineline' }, t); };
    var body =
      s === 'elsewhere' ? line(props.where || 'On Ana’s Mac') :
      s === 'queued' ? h('div', { className: 'bae-a-dlrow' }, line('Queued to pin'), btn('close', 'Cancel')) :
      s === 'downloading' ? [h(AndroidProgress, { key: 'p', name: 'Pinning', value: Number(props.progress || 0.4) }), h('span', { key: 'b', className: 'bae-a-fine' }, 'Pinning · ' + (props.bytes || '120 MB of 300 MB')), h('div', { key: 'c' }, btn('close', 'Cancel pinning'))] :
      s === 'failed' ? [h(AndroidNotice, { key: 'e', quiet: true, tone: 'danger', title: 'Could not pin', secondary: 'Details', action: 'Retry' }), h('div', { key: 'r' }, btn('close', 'Cancel'))] :
      s === 'downloaded' ? h('div', { className: 'bae-a-dlrow' }, h(Glyph, { name: 'pin', size: 16, className: 'bae-muted' }), line('Pinned'), btn('unpin', 'Unpin')) :
      btn('pin', 'Pin');
    return h('div', { className: 'bae-a-dl' }, body);
  }

  // The album page's upload line, as ReleaseUploadLine draws it: the phase core words, Queued to upload while the files wait; a missing original (`missing`) as a quiet warning notice naming it.
  function AndroidUploadLine(props) {
    return h('div', { className: 'bae-a-dl' }, h('span', { className: 'bae-a-fineline' }, props.phase || 'Queued to upload'),
      props.missing ? h(AndroidNotice, { quiet: true, tone: 'warning', title: props.missing + ' is missing', subtitle: 'Restore it to finish uploading.' }) : null);
  }
