  // ---- Search ----
  function MacSearchCard(props) {
    var style = { position: 'absolute', top: Number(props.top != null ? props.top : 60), right: Number(props.right != null ? props.right : 56), width: 440, zIndex: 6 };
    return h('div', { className: 'bae-m-searchcard', style: style }, props.empty ? h('div', { className: 'bae-m-searchcard-empty' }, props.empty) : props.children);
  }
  function MacSearchSection(props) { return h('div', { className: 'bae-m-searchsection' }, props.title); }
  // One row of a library list on the Mac: Search's results, the artist and composer lists, the composer and work pages' rows.
  // A 44pt cover, the title over a second line 2pt below (it holds its height when empty), and `trailing` (a track's duration). `hover` and `selected` fill it;
  // `loading` is the row before it arrives. `credit` is the text-only row of a credit or a recording, which opens nothing. `bleed` lets the fill reach 8pt past
  // the text, for a row on a page rather than in a list; `part` indents it 16pt under its group's heading work; `top` is the gap above it.
  function MacLibraryRow(props) {
    var ml = (props.bleed && !props.credit ? -8 : 0) + (props.part ? 16 : 0);
    var style = { marginTop: props.top ? Number(props.top) : null, marginLeft: ml || null };
    if (props.loading) return h('div', { className: 'bae-m-librow', style: style }, h('div', { className: 'bae-ph', style: { width: 44, height: 44, borderRadius: 6, flex: 'none' } }),
      h('div', { className: 'bae-m-librow-text' }, h('div', { className: 'bae-ph', style: { width: 140, height: 11 } }), h('div', { className: 'bae-ph', style: { width: 80, height: 10 } })));
    return h('div', { className: cx('bae-m-librow', props.credit && 'bae-m-librow-credit', props.bleed && !props.credit && 'bae-m-librow-bleed', props.hover && 'bae-m-librow-hover', props.selected && 'bae-m-librow-selected'), style: style },
      props.credit ? null : h(Artwork, { title: props.art || props.title, size: 44, radius: 6, missing: props.missing }),
      h('div', { className: 'bae-m-librow-text' }, h('div', { className: 'bae-m-librow-title' }, props.title), h('div', { className: 'bae-m-librow-sub' }, props.subtitle || ' ')),
      props.trailing ? h('span', { className: 'bae-m-librow-trail' }, props.trailing) : null);
  }
  function AndroidSearchSection(props) { return h('div', { className: 'bae-a-searchsection' }, props.title); }
  // One row of a library list: Search's results, the artist and composer lists, the composer and work pages' rows.
  function AndroidLibraryRow(props) {
    return h('div', { className: cx('bae-a-librow', props.part && 'bae-a-librow-part', props.inset && 'bae-a-librow-inset') },
      props.noArt ? null : h(Artwork, { title: props.art || props.title, size: 48, radius: 6, missing: props.missing }),
      h('div', { className: 'bae-a-librow-text' }, h('div', { className: 'bae-a-librow-title' }, props.title), props.subtitle ? h('div', { className: 'bae-a-librow-sub' }, props.subtitle) : null),
      props.trailing ? h('span', { className: 'bae-a-librow-trail' }, props.trailing) : null);
  }

  // ---- Queue and playback (macOS) ----
  function MacQueuePanel(props) {
    return h('aside', { className: 'bae-m-queue' },
      h('div', { className: 'bae-m-queue-head' }, h('h2', null, 'Queue'), h(MacCloseButton)),
      props.title ? h('div', { className: 'bae-m-queue-np' },
        h(Artwork, { title: props.album || props.title, size: 56, radius: 6, shadow: 'bar' }),
        h('div', { className: 'bae-m-queue-np-text' },
          h('div', { className: 'bae-m-queue-np-eyebrow' }, 'Now playing'),
          h('div', { className: 'bae-m-np-title' }, props.title), h('div', { className: 'bae-m-np-sub' }, props.artist),
          h('div', { className: 'bae-m-queue-strip' }, h('div', { className: 'bae-m-queue-strip-bar' }, h('div', { style: { width: (Number(props.position || 0) * 100) + '%' } })), h('span', null, props.elapsed || ''))),
        h('span', { className: 'bae-m-queue-np-play' }, h(Glyph, { set: 'mac', name: props.playing ? 'pause' : 'play', size: 13 }))) : null,
      props.empty ? h('div', { className: 'bae-m-queue-empty' }, h('div', { className: 'bae-m-cu-title' }, 'Queue is empty'), h('div', { className: 'bae-m-cu-desc' }, 'Drag tracks here or play an album'))
        : h('div', { className: 'bae-m-queue-body' }, props.children));
  }
  function MacQueueSection(props) {
    return h('div', { className: 'bae-m-queue-section' },
      h('span', { className: 'bae-m-queue-section-title' }, props.title),
      props.clear ? h('span', { className: 'bae-m-queue-clear' }, 'Clear') : null,
      props.shuffle != null ? h('span', { className: cx('bae-m-queue-shuffle', props.shuffle && 'bae-m-queue-shuffle-on'), 'aria-label': props.shuffle ? 'Turn off shuffle' : 'Shuffle' }, h(Glyph, { set: 'mac', name: 'shuffle', size: 11 })) : null);
  }
  function MacQueuePlaceholderRow() {
    return h('div', { className: 'bae-m-qrow' }, h('div', { className: 'bae-ph', style: { width: 48, height: 48, borderRadius: 6 } }),
      h('div', { className: 'bae-m-qrow-text', style: { gap: 4 } }, h('div', { className: 'bae-ph', style: { width: 140, height: 12 } }), h('div', { className: 'bae-ph', style: { width: 90, height: 10 } }), h('div', { className: 'bae-ph', style: { width: 120, height: 10 } })));
  }
  function MacQueueInsertLine() { return h('div', { className: 'bae-m-queue-insert' }); }

  function MacSidePause(props) {
    var disc = props.kind === 'disc';
    return h('div', { className: 'bae-m-scrim' }, h('div', { className: 'bae-m-sidepause', role: 'dialog' },
      h('div', { className: 'bae-m-sidepause-title' }, (disc ? 'Disc ' : 'Side ') + (props.label || (disc ? '1' : 'A')) + ' ended'),
      h('div', { className: 'bae-muted' }, 'Playback is paused. Press Play to continue.'),
      props.countdown ? h('div', { className: 'bae-m-sidepause-count' }, 'The next ' + (disc ? 'disc' : 'side') + ' starts in ' + props.countdown + ' seconds.') : null,
      h(MacCheckbox, { label: disc ? 'Pause between discs' : 'Pause between sides', on: !props.unchecked }),
      props.unchecked ? h('div', { className: 'bae-muted' }, 'You can turn this back on in Settings › Playback.') : null,
      h('div', { className: 'bae-m-sidepause-buttons' }, h(MacButton, null, 'Close'), h(MacButton, { kind: 'primary' }, 'Play'))));
  }

  // A receiver, wherever bae lists them (the cast popover, Settings → Casting): its kind's icon, its name, then its kind, or Casting in the accent for the one being cast to, which ends in Disconnect. Clicking any other casts to it. One row per receiver. `menu` (Settings → Casting) puts a ⋯ at the end of every row in place of Disconnect: Details… and, on the one being cast to, Disconnect.
  var CAST_KIND = { cast: 'Chromecast', airplay: 'AirPlay', upnp: 'UPnP' };
  function MacCastRow(props) {
    var glyph = { cast: 'cast', airplay: 'airplay', upnp: 'tv' }[props.kind || 'cast'];
    return h('div', { className: 'bae-m-castrow' }, h(MacItem, { lead: h(Glyph, { set: 'mac', name: glyph, size: 15, className: props.active ? 'bae-accent' : 'bae-muted' }), trail: props.menu ? h(MacMoreMenu, {}) : props.active ? h(MacButton, { kind: 'borderless', size: 'small' }, 'Disconnect') : null },
      h('span', { className: 'bae-m-castpop-text' }, h('span', { className: 'bae-m-castpop-name' }, props.name), h('span', { className: props.active ? 'bae-m-castpop-state' : 'bae-m-castpop-kind' }, props.active ? 'Casting' : CAST_KIND[props.kind || 'cast']))));
  }
  function MacCastPopover(props) {
    var devices = String(props.devices || '').split(',').filter(Boolean).map(function (d) { var p = d.trim().split(':'); return { name: p[0], kind: p[1] || 'cast' }; });
    return h('div', { className: 'bae-m-castpop', style: { position: 'absolute', right: Number(props.right != null ? props.right : 120), bottom: Number(props.bottom != null ? props.bottom : 92) } },
      props.connecting ? h('div', { className: 'bae-m-castpop-wait' }, h(MacSpinner, { size: 14 }), 'Connecting to ' + props.connecting + '…') : null,
      devices.length ? devices.map(function (d, i) { return h(MacCastRow, { key: i, name: d.name, kind: d.kind, active: d.name === props.casting }); })
        : (props.connecting || props.casting ? null : h('div', { className: 'bae-muted' }, 'No Cast devices found')));
  }

  // ---- Queue and playback (Android) ----
  function AndroidSheet(props) {
    return h('div', { className: 'bae-a-scrim' }, h('div', { className: 'bae-a-sheet', style: props.top != null ? { top: Number(props.top) } : null }, h('div', { className: 'bae-a-sheet-handle' }), props.title ? h('h2', { className: 'bae-a-sheet-title' }, props.title) : null, h('div', { className: 'bae-a-sheet-body' }, props.children)));
  }
  function AndroidQueueLabel(props) {
    return h('div', { className: 'bae-a-queuelabel' }, h('span', { className: 'bae-a-eyebrow', style: { paddingTop: 0, flex: 1 } }, props.title),
      props.clear ? h(AndroidButton, { kind: 'text' }, 'Clear') : null,
      props.shuffle != null ? h(AndroidIconButton, { icon: 'shuffle', label: props.shuffle ? 'Turn off shuffle' : 'Shuffle', size: 20, tone: props.shuffle ? 'accent' : 'muted' }) : null);
  }
  function AndroidQueueRow(props) {
    return h('div', { className: cx('bae-a-qrow', props.dragging && 'bae-a-qrow-drag') }, h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }),
      h('div', { className: 'bae-a-qrow-text' }, h('div', { className: cx('bae-a-track-title', props.current && 'bae-accent') }, props.title), h('div', { className: 'bae-a-track-artist' }, props.artist), props.current ? null : h('div', { className: 'bae-a-track-artist' }, props.album)),
      props.current ? null : [h('span', { key: 'd', className: 'bae-a-track-time', style: { padding: '0 8px' } }, props.duration), h(AndroidIconButton, { key: 'x', icon: 'close', label: 'Remove', tone: 'muted' }), h(Glyph, { key: 'h', name: 'drag', size: 20, className: 'bae-muted' })]);
  }
  // A queued track not loaded yet: the cover's tile and a bar for each of the row's three lines.
  function AndroidQueuePlaceholderRow() {
    return h('div', { className: 'bae-a-qrow' }, h('div', { className: 'bae-ph', style: { width: 48, height: 48, borderRadius: 6, flex: 'none' } }),
      h('div', { className: 'bae-a-qrow-text', style: { display: 'flex', flexDirection: 'column', gap: 6 } }, h('div', { className: 'bae-ph', style: { width: 160, height: 12 } }), h('div', { className: 'bae-ph', style: { width: 100, height: 10 } }), h('div', { className: 'bae-ph', style: { width: 120, height: 10 } })));
  }
  function AndroidFullPlayer(props) {
    return h('div', { className: 'bae-a-fullplayer' },
      h(AndroidIconButton, { icon: 'down', label: 'Collapse' }),
      h(Artwork, { title: props.album || props.title, radius: 10 }),
      h('div', { className: 'bae-a-fp-title' }, props.title), h('div', { className: 'bae-a-fp-artist' }, props.artist),
      h(AndroidSeekBar, { large: true, position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration }),
      h('div', { className: 'bae-a-fp-transport' }, h(AndroidIconButton, { box: 56, icon: 'previous', size: 36, label: 'Previous' }), h(AndroidPlayPause, { playing: props.playing, waiting: props.waiting, size: 48 }), h(AndroidIconButton, { box: 56, icon: 'next', size: 36, label: 'Next' })),
      h('div', { className: 'bae-a-fp-repeat' }, h(AndroidIconButton, { icon: 'repeat', label: 'Repeat mode', tone: props.repeat ? 'accent' : 'muted' })),
      h('div', { className: 'bae-a-fp-volume' }, h(AndroidIconButton, { icon: 'nowplaying', label: 'Mute', tone: 'muted' }), h('div', { className: 'bae-a-fp-slider' }, h('div', { style: { width: '70%' } }), h('span', { style: { left: '70%' } }))));
  }
  function AndroidCastRow(props) {
    if (props.waiting) return h('div', { className: 'bae-a-castrow' }, h(AndroidSpinner, { size: 20 }), h('span', { className: 'bae-muted', style: { fontSize: 14 } }, props.waiting));
    // A receiver, in the picker and on the Casting page alike: under its name its kind, or Casting for the one being cast to, which ends in Disconnect. Tapping any other casts to it. No separate "Casting to" row, and no check: the row says it. `menu` (the Casting page) ends every row in ⋮ in place of Disconnect: Details… and, on the one being cast to, Disconnect; `menuOpen` draws that menu open under it.
    var icon = { cast: 'cast', airplay: 'speakerbox', upnp: 'tv' }[props.kind || 'cast'];
    return h('div', { className: 'bae-a-castrow' }, h(Glyph, { name: icon, size: 24, className: props.active ? 'bae-accent' : 'bae-muted' }),
      h('span', { className: 'bae-a-castrow-text' }, h('span', { className: 'bae-a-castrow-name' }, props.name), h('span', { className: props.active ? 'bae-a-castrow-state' : 'bae-a-castrow-kind' }, props.active ? 'Casting' : CAST_KIND[props.kind || 'cast'])),
      props.menu ? h(AndroidMoreMenu, { open: props.menuOpen, items: props.active ? 'Details…,Disconnect' : 'Details…' }) : props.active ? h(AndroidButton, { kind: 'text' }, 'Disconnect') : null);
  }
  function AndroidCheckbox(props) {
    return h('div', { className: 'bae-a-checkrow' }, h('span', { className: cx('bae-a-checkbox', props.on && 'bae-a-checkbox-on') }, props.on ? h(Glyph, { name: 'check', size: 16 }) : null), h('span', null, props.label));
  }


  // ---- Composer and work pages (macOS) ----
  function MacSectionHeader(props) {
    return h('div', { className: 'bae-m-sechead', style: { marginTop: Number(props.top || 0), marginBottom: Number(props.bottom || 0) } }, props.title);
  }
  // A work's detail: its title, then its sections (MacSectionHeader and MacLibraryRow) 12pt apart.
  function MacWorkDetail(props) {
    return h('div', { className: 'bae-m-work' }, h('div', { className: 'bae-m-work-title' }, props.title), props.children);
  }
  function MacHairline() { return h('div', { className: 'bae-m-hairline' }); }
  function MacContentUnavailable(props) {
    return h('div', { className: 'bae-m-cu' }, h('div', { className: 'bae-m-cu-title' }, props.title), props.description ? h('div', { className: 'bae-m-cu-desc' }, props.description) : null);
  }
