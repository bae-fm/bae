  // ---- Library and playback ----

  // A stand-in cover: each title gets its own two-color square, so boards read like a library.
  // `missing` is a cover the release doesn't have: a plain tile, no glyph.
  var boardArtwork = /* board artwork */ {};
  function hueOf(s) { var x = 0; s = String(s || ''); for (var i = 0; i < s.length; i++) x = (x * 31 + s.charCodeAt(i)) % 360; return x; }
  // `size` and `radius` are pixels or a token's name (`size-row-artwork`, `radius-artwork`).
  function Artwork(props) {
    var size = props.size == null ? null : measure(props.size);
    var name = props.seed || props.title;
    var style = { borderRadius: measure(props.radius != null ? props.radius : 'radius-cover') };
    if (size) { style.width = size; style.height = size; }
    if (props.round) style.borderRadius = '50%';
    if (!props.missing) {
      if (props.hue == null && Object.hasOwn(boardArtwork, name)) {
        style.backgroundImage = 'url(' + boardArtwork[name] + ')';
        style.backgroundSize = '100% 100%';
      } else {
        var hue = props.hue != null ? Number(props.hue) : hueOf(name);
        style.backgroundImage = 'linear-gradient(135deg, hsl(' + hue + ' 42% 52%), hsl(' + ((hue + 48) % 360) + ' 46% 24%))';
      }
    }
    return h('div', { className: cx('bae-art', props.missing && 'bae-art-missing', props.shadow && 'bae-art-shadow-' + props.shadow, props.ring && 'bae-art-ring', props.dimmed && 'bae-art-dimmed', props.className), style: style, role: props.label ? 'img' : null, 'aria-label': props.label || null, 'aria-hidden': props.label ? null : true }, props.children);
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
    var size = measure(props.size || 48), radius = measure(props.radius != null ? props.radius : 'radius-artwork');
    if (covers.length < 4) return h(Artwork, { title: covers[0], size: props.size || 48, radius: props.radius != null ? props.radius : 'radius-artwork', missing: !covers.length });
    // Four covers, each filling a quarter of the picture's two-by-two grid.
    return h('div', { className: 'bae-playlist-pic', style: { width: size, height: size, borderRadius: radius }, 'aria-hidden': true },
      covers.slice(0, 4).map(function (t, i) { return h(Artwork, { key: i, title: t, radius: 0 }); }));
  }

  // The Mac app's symbols, each the outline of the SF Symbol the app draws, in its weight, at the 24 point size the Glyph's 24 by 24 box stands for, centred on the symbol's own box. A Glyph of size s is the symbol at s points, and it may reach past its box, as the symbol does past its frame.
  // play play.fill, pause pause.fill, previous backward.fill, next forward.fill, shuffle shuffle, repeat repeat, queue list.bullet, speaker speaker.wave.2.fill, cast hifispeaker, search magnifyingglass, ellipsis ellipsis, warning exclamationmark.circle, close xmark, airplay airplayvideo, tv tv, down chevron.down, add plus, sidebar sidebar.trailing, lock lock.fill, people person.2.fill, removefill xmark.circle.fill.
  var SF = {
    play: 'M5.59 2.27C5 2.43 4.62 2.89 4.52 3.56C4.49 3.8 4.49 20.32 4.52 20.56C4.7 21.77 5.79 22.24 6.99 21.62C7.4 21.4 20.77 13.59 21.08 13.38C21.65 13 21.92 12.66 21.98 12.24C22.06 11.66 21.8 11.22 21.08 10.74C20.77 10.54 7.18 2.6 6.9 2.46C6.48 2.25 5.95 2.18 5.59 2.27Z',
    pause: 'M5.64 2.56C4.99 2.68 4.63 3.07 4.52 3.75C4.51 3.86 4.5 6.75 4.51 12.32C4.51 20.37 4.52 20.74 4.56 20.89C4.72 21.41 5 21.7 5.52 21.84C5.77 21.91 9 21.91 9.25 21.84C9.8 21.69 10.14 21.34 10.24 20.79C10.3 20.49 10.3 3.89 10.23 3.6C10.12 3.02 9.76 2.67 9.16 2.56C8.87 2.51 5.91 2.51 5.64 2.56ZM14.24 2.54C13.62 2.63 13.23 2.98 13.07 3.56C13.02 3.76 13.02 3.82 13.02 12.21C13.02 20.61 13.02 20.67 13.07 20.87C13.21 21.39 13.51 21.7 14.05 21.84C14.3 21.91 17.53 21.91 17.77 21.84C18.3 21.7 18.6 21.38 18.74 20.85C18.81 20.6 18.81 3.8 18.74 3.55C18.6 3.03 18.32 2.73 17.81 2.59C17.68 2.54 17.52 2.54 15.99 2.54C15.07 2.53 14.28 2.54 14.24 2.54Z',
    previous: 'M8.61 2.87C8.16 2.95 8.09 2.98 6.07 4.15C5.12 4.71 2.27 6.37 -0.25 7.84C-2.77 9.3 -4.95 10.59 -5.09 10.68C-5.57 11.01 -5.86 11.32 -5.98 11.68C-6.05 11.89 -6.06 12.34 -5.99 12.55C-5.85 12.96 -5.51 13.32 -4.91 13.7C-4.4 14.01 7.79 21.09 7.97 21.18C8.53 21.45 9.14 21.47 9.61 21.24C10.02 21.05 10.28 20.71 10.43 20.19C10.49 20.02 10.49 19.98 10.5 16.32L10.51 12.62L10.6 12.79C10.71 13.02 11.05 13.36 11.39 13.58C11.86 13.89 24.33 21.13 24.54 21.22C24.83 21.34 25.01 21.38 25.29 21.39C26.16 21.44 26.8 20.9 26.95 20C26.97 19.89 26.98 17.4 26.98 12.11C26.98 4.55 26.97 4.38 26.92 4.15C26.69 2.98 25.53 2.51 24.35 3.12C24.02 3.29 11.57 10.55 11.34 10.71C11.03 10.92 10.71 11.24 10.6 11.45L10.51 11.62L10.5 7.94L10.49 4.26L10.43 4.06C10.26 3.46 9.88 3.05 9.35 2.91C9.18 2.86 8.78 2.84 8.61 2.87Z',
    next: 'M-2.2 2.88C-2.82 2.99 -3.26 3.41 -3.43 4.07L-3.49 4.29L-3.49 12.12L-3.49 19.96L-3.43 20.17C-3.22 20.98 -2.62 21.44 -1.81 21.39C-1.51 21.38 -1.24 21.31 -0.97 21.18C-0.79 21.09 11.39 14.01 11.91 13.7C12.38 13.4 12.75 13.06 12.89 12.78L12.97 12.62L12.98 16.29L12.99 19.96L13.05 20.17C13.2 20.76 13.57 21.17 14.08 21.33C14.46 21.45 14.99 21.41 15.42 21.22C15.62 21.13 28.16 13.85 28.5 13.62C29.19 13.16 29.46 12.79 29.51 12.26C29.57 11.67 29.27 11.15 28.61 10.71C28.35 10.53 15.89 3.27 15.58 3.11C15.22 2.93 14.97 2.87 14.59 2.87C14.3 2.87 14.24 2.88 14.06 2.93C13.55 3.1 13.2 3.49 13.05 4.08L12.99 4.29L12.98 7.96C12.97 10.91 12.96 11.62 12.93 11.55C12.84 11.3 12.51 10.96 12.06 10.66C11.73 10.44 -0.74 3.18 -0.97 3.07C-1.35 2.9 -1.86 2.81 -2.2 2.88Z',
    shuffle: 'M19.92 1.31C19.68 1.38 19.49 1.55 19.4 1.79L19.34 1.94L19.33 3.43L19.32 4.92L17.78 4.93C16.12 4.95 16 4.95 15.38 5.09C14.63 5.26 13.95 5.59 13.3 6.12C12.85 6.48 12.43 6.94 10.9 8.76C9.67 10.22 9.39 10.55 9.36 10.52C9.35 10.5 8.73 9.77 7.99 8.89C7.25 8.01 6.53 7.16 6.4 7.01C5.32 5.79 4.33 5.23 2.9 5.05C2.77 5.03 2 5.01 0.93 5.01C-1.02 4.99 -1.04 5 -1.37 5.15C-2.12 5.5 -2.22 6.63 -1.52 7.09C-1.18 7.32 -1.31 7.31 0.62 7.33C1.77 7.34 2.4 7.35 2.54 7.37C3.33 7.5 3.96 7.83 4.61 8.49C4.82 8.7 7.83 12.23 7.88 12.33C7.9 12.37 4.94 15.86 4.62 16.18C3.89 16.91 3.26 17.23 2.35 17.32C2.2 17.34 1.46 17.35 0.59 17.35C-0.43 17.35 -0.96 17.36 -1.05 17.38C-1.6 17.49 -2 17.96 -2 18.49C-2 19.01 -1.71 19.41 -1.21 19.61L-1.06 19.66L0.81 19.66C2.19 19.66 2.75 19.66 2.93 19.63C4.23 19.45 5.11 19.01 6.1 17.98C6.27 17.81 7.08 16.88 7.88 15.91C8.7 14.95 9.37 14.15 9.38 14.14C9.38 14.13 10.06 14.92 10.88 15.89C12.7 18.06 13.03 18.41 13.74 18.89C14.18 19.19 14.8 19.46 15.34 19.59C15.92 19.72 16.06 19.73 17.75 19.74L19.33 19.75L19.33 21.11C19.33 22.8 19.34 22.9 19.56 23.14C19.71 23.31 19.91 23.39 20.23 23.39C20.66 23.39 20.51 23.49 23.19 21.26C24.48 20.19 25.57 19.27 25.62 19.22C25.91 18.91 25.92 18.44 25.64 18.13C25.45 17.92 20.89 14.14 20.72 14.05C20.24 13.8 19.64 13.94 19.44 14.36C19.34 14.56 19.33 14.77 19.33 16.14L19.33 17.42L18.17 17.42C17.53 17.42 16.85 17.41 16.66 17.4C15.75 17.34 15.13 17.08 14.46 16.47C14.14 16.18 14.08 16.12 12.41 14.13C11.11 12.59 10.91 12.34 10.93 12.3C11.25 11.89 14.04 8.61 14.23 8.43C14.86 7.79 15.41 7.48 16.16 7.34C16.36 7.3 16.59 7.29 17.86 7.28L19.33 7.27L19.33 8.53C19.33 9.9 19.34 10.08 19.43 10.28C19.63 10.7 20.2 10.86 20.69 10.63C20.8 10.58 25.3 6.87 25.61 6.58C25.8 6.39 25.89 6.09 25.83 5.83C25.75 5.53 25.83 5.6 23.24 3.44C20.54 1.2 20.68 1.29 20.27 1.28C20.14 1.28 19.99 1.29 19.92 1.31Z',
    repeat: 'M14.67 2C14.4 2.05 14.17 2.21 14.06 2.45C14.02 2.56 14.01 2.6 14.01 3.97L14 5.38L9.42 5.38C4.72 5.38 4.43 5.38 3.76 5.48C1.34 5.82 -0.31 7.16 -0.82 9.19C-0.96 9.73 -0.98 9.93 -0.99 10.98C-1 12.04 -1 12.12 -0.87 12.38C-0.65 12.82 -0.11 13.09 0.4 13C0.89 12.92 1.29 12.54 1.37 12.07C1.39 11.98 1.4 11.61 1.4 11.23C1.4 10.44 1.43 10.16 1.57 9.75C1.88 8.79 2.65 8.09 3.7 7.81C4.27 7.66 3.9 7.67 9.17 7.66L14 7.65L14 9.05C14 10.61 14 10.57 14.17 10.81C14.23 10.89 14.29 10.94 14.41 11C14.58 11.09 14.6 11.09 14.88 11.09C15.15 11.09 15.17 11.08 15.33 11.01C15.52 10.92 20.07 7.3 20.28 7.07C20.58 6.74 20.57 6.29 20.25 5.98C19.94 5.7 15.52 2.18 15.38 2.1C15.13 1.98 14.94 1.95 14.67 2ZM23.24 11.98C22.99 12.04 22.7 12.25 22.55 12.48C22.39 12.72 22.38 12.83 22.38 13.53C22.38 14.49 22.32 14.94 22.12 15.41C21.78 16.24 21.05 16.87 20.11 17.13C19.55 17.3 19.94 17.28 14.62 17.29L9.77 17.3L9.77 15.96C9.77 14.59 9.77 14.46 9.67 14.27C9.47 13.88 8.88 13.73 8.39 13.96C8.27 14.02 3.72 17.65 3.49 17.87C3.19 18.17 3.18 18.64 3.48 18.95C3.62 19.09 8.14 22.71 8.31 22.82C8.75 23.09 9.42 22.99 9.64 22.61C9.77 22.39 9.77 22.31 9.77 20.89L9.77 19.58L14.63 19.57C18.21 19.56 19.54 19.55 19.7 19.53C21.24 19.31 22.24 18.91 23.15 18.14C23.86 17.53 24.42 16.59 24.62 15.62C24.73 15.12 24.75 14.85 24.75 13.85C24.75 12.97 24.75 12.89 24.7 12.74C24.6 12.41 24.29 12.11 23.93 12C23.76 11.95 23.42 11.93 23.24 11.98Z',
    queue: 'M0.7 3.28C-0.51 3.65 -0.9 5.13 -0.01 6.01C0.86 6.89 2.3 6.55 2.71 5.38C2.8 5.11 2.82 4.69 2.74 4.44C2.58 3.9 2.16 3.46 1.65 3.29C1.39 3.21 0.95 3.21 0.7 3.28ZM6.38 3.79C5.94 3.88 5.59 4.26 5.53 4.7C5.45 5.26 5.8 5.8 6.34 5.94C6.56 5.99 22.59 5.99 22.81 5.94C23.14 5.85 23.4 5.64 23.55 5.33C23.64 5.14 23.64 5.13 23.64 4.86C23.64 4.6 23.64 4.58 23.55 4.4C23.39 4.07 23.09 3.84 22.75 3.77C22.54 3.73 6.55 3.74 6.38 3.79ZM0.92 10.43C0.55 10.48 0.27 10.63 -0.02 10.91C-0.88 11.78 -0.54 13.2 0.63 13.62C0.9 13.72 1.4 13.72 1.67 13.62C2.19 13.43 2.58 13.02 2.74 12.49C2.81 12.25 2.8 11.82 2.71 11.56C2.47 10.81 1.68 10.31 0.92 10.43ZM6.32 10.98C5.91 11.1 5.59 11.48 5.53 11.9C5.45 12.49 5.84 13.04 6.42 13.15C6.62 13.19 22.58 13.18 22.76 13.15C23.4 13.01 23.79 12.35 23.6 11.73C23.51 11.42 23.26 11.15 22.92 11.01C22.82 10.96 22.61 10.96 14.62 10.96C8.06 10.95 6.41 10.96 6.32 10.98ZM0.81 17.64C0.19 17.77 -0.33 18.29 -0.46 18.91C-0.65 19.79 -0.07 20.67 0.81 20.86C1.7 21.05 2.58 20.46 2.76 19.57C2.99 18.43 1.96 17.4 0.81 17.64ZM6.27 18.2C5.27 18.52 5.27 19.98 6.29 20.31C6.41 20.35 7.07 20.35 14.62 20.34L22.84 20.34L23 20.26C23.93 19.82 23.84 18.5 22.87 18.2C22.73 18.15 22.48 18.15 14.57 18.15C6.8 18.15 6.41 18.16 6.27 18.2Z',
    speaker: 'M10.58 2.35C10.35 2.4 10.04 2.55 9.76 2.76C9.54 2.91 9.39 3.05 6.85 5.34C5.52 6.54 4.4 7.54 4.35 7.58L4.27 7.64L2.32 7.65C0.61 7.66 0.34 7.67 0.14 7.71C-1.05 7.94 -1.68 8.63 -1.82 9.86C-1.84 10.04 -1.85 10.84 -1.84 12.57C-1.84 15.02 -1.84 15.04 -1.78 15.29C-1.55 16.3 -1.01 16.84 0.06 17.1C0.16 17.12 0.72 17.14 2.21 17.15C3.95 17.16 4.25 17.17 4.3 17.2C4.34 17.22 5.51 18.27 6.89 19.53C8.28 20.79 9.5 21.89 9.61 21.98C10.3 22.49 11.06 22.61 11.7 22.31C11.94 22.19 12.21 21.93 12.32 21.7C12.49 21.35 12.48 22.12 12.48 12.42C12.48 3.75 12.48 3.63 12.43 3.44C12.29 2.9 11.88 2.5 11.34 2.36C11.18 2.32 10.76 2.31 10.58 2.35ZM21.65 4.24C20.99 4.42 20.69 5.04 20.95 5.67C20.98 5.73 21.11 5.95 21.25 6.16C21.56 6.64 21.77 7 21.97 7.41C23.06 9.71 23.35 12.44 22.77 14.99C22.47 16.31 21.95 17.55 21.21 18.67C20.84 19.22 20.77 19.61 20.95 19.99C21.17 20.41 21.71 20.65 22.19 20.54C22.51 20.46 22.7 20.31 23 19.89C24.23 18.12 25 15.98 25.23 13.59C25.27 13.15 25.27 11.59 25.23 11.16C25.04 9.3 24.57 7.64 23.8 6.18C23.45 5.52 22.93 4.73 22.7 4.5C22.43 4.24 22.02 4.14 21.65 4.24ZM17.08 7.23C16.59 7.3 16.19 7.72 16.16 8.18C16.13 8.5 16.21 8.72 16.52 9.18C16.96 9.83 17.25 10.59 17.38 11.46C17.45 11.96 17.45 12.79 17.38 13.29C17.25 14.15 16.95 14.94 16.54 15.56C16.32 15.89 16.21 16.11 16.17 16.33C16.09 16.79 16.32 17.22 16.77 17.45C16.95 17.53 16.97 17.54 17.25 17.54C17.54 17.54 17.54 17.53 17.73 17.45C17.97 17.32 18.15 17.13 18.42 16.72C19.02 15.81 19.42 14.67 19.59 13.41C19.65 12.96 19.65 11.82 19.59 11.36C19.4 9.94 18.92 8.69 18.21 7.74C17.9 7.33 17.52 7.16 17.08 7.23Z',
    cast: 'M5.56 -0.3C3.72 -0.16 2.69 0.86 2.52 2.71C2.51 2.92 2.5 5.84 2.51 12.25L2.52 21.49L2.57 21.76C2.65 22.2 2.74 22.46 2.9 22.79C3.32 23.65 4.02 24.13 5.18 24.34C5.4 24.38 5.82 24.39 11.59 24.39C15.99 24.4 17.85 24.39 18.08 24.38C19.86 24.22 20.83 23.27 21.05 21.5C21.1 21.11 21.1 2.93 21.05 2.57C20.85 1.06 20.14 0.18 18.85 -0.16C18.26 -0.32 18.7 -0.31 11.96 -0.32C8.59 -0.32 5.71 -0.31 5.56 -0.3ZM17.91 2.03C18.36 2.17 18.62 2.43 18.74 2.88C18.79 3.03 18.79 3.38 18.79 12.04C18.79 20.67 18.79 21.05 18.74 21.2C18.61 21.67 18.3 21.96 17.83 22.06C17.58 22.12 6.02 22.12 5.77 22.06C5.38 21.98 5.09 21.77 4.95 21.46C4.79 21.12 4.8 21.83 4.8 11.96L4.81 2.96L4.87 2.79C5.02 2.35 5.34 2.08 5.85 2C5.93 1.99 8.45 1.98 11.88 1.98C17.48 1.99 17.77 1.99 17.91 2.03ZM11.31 3.77C11.1 3.81 10.77 3.93 10.55 4.04C9.23 4.7 8.71 6.35 9.39 7.68C9.66 8.2 10.07 8.59 10.59 8.85C12.35 9.72 14.4 8.52 14.49 6.59C14.55 5.29 13.72 4.16 12.48 3.83C12.21 3.76 11.56 3.73 11.31 3.77ZM12.06 5.21C12.85 5.38 13.27 6.26 12.91 6.99C12.48 7.85 11.27 7.93 10.75 7.11C10.56 6.83 10.5 6.43 10.59 6.09C10.75 5.46 11.4 5.07 12.06 5.21ZM11.07 10.91C8.89 11.25 7.25 13.02 7.07 15.25C6.95 16.94 7.76 18.61 9.16 19.54C10.67 20.53 12.61 20.59 14.16 19.7C15.32 19.04 16.16 17.87 16.42 16.58C16.86 14.44 15.85 12.34 13.91 11.37C13.45 11.13 12.97 10.99 12.45 10.91C12.12 10.86 11.39 10.87 11.07 10.91ZM12.24 13.74C13.43 14 14.03 15.32 13.48 16.45C13.32 16.77 13 17.11 12.68 17.27C11.91 17.65 11.03 17.51 10.44 16.91C10.05 16.51 9.88 16.05 9.91 15.48C9.96 14.29 11.05 13.48 12.24 13.74Z',
    search: 'M9.36 0.39C9.31 0.39 9.12 0.41 8.95 0.43C5.98 0.68 3.18 2.45 1.6 5.05C0.86 6.27 0.41 7.61 0.25 9.07C0.2 9.51 0.2 10.68 0.26 11.14C0.53 13.37 1.47 15.32 3.04 16.89C4.62 18.47 6.54 19.4 8.82 19.68C9.25 19.73 10.6 19.73 11.02 19.67C12.38 19.48 13.58 19.07 14.68 18.41L14.99 18.23L17.64 20.87C19.68 22.91 20.32 23.54 20.44 23.61C21.08 23.99 21.94 23.99 22.57 23.62C23.53 23.05 23.8 21.68 23.15 20.71C23.11 20.64 21.9 19.42 20.48 17.99L17.89 15.41L18.03 15.19C20.44 11.53 19.96 6.59 16.88 3.37C15.3 1.73 13.23 0.7 10.95 0.44C10.63 0.4 9.57 0.37 9.36 0.39ZM10.89 3.22C12.4 3.45 13.69 4.11 14.77 5.2C15.61 6.03 16.16 6.95 16.51 8.09C16.84 9.17 16.88 10.5 16.61 11.64C16.32 12.89 15.68 14.02 14.77 14.94C12.05 17.66 7.68 17.65 4.98 14.92C3.91 13.84 3.27 12.55 3.03 11.01C2.97 10.61 2.97 9.52 3.03 9.11C3.19 8.07 3.55 7.13 4.09 6.29C5.25 4.53 7.11 3.39 9.21 3.18C9.58 3.14 10.51 3.16 10.89 3.22Z',
    ellipsis: 'M2.62 10.18C1.67 10.35 0.91 10.99 0.67 11.84C0.6 12.09 0.59 12.6 0.66 12.84C0.76 13.2 0.89 13.46 1.11 13.72C2.23 15.01 4.42 14.85 5.27 13.41C5.61 12.84 5.64 12.11 5.36 11.51C5.08 10.89 4.48 10.42 3.75 10.22C3.54 10.16 2.84 10.13 2.62 10.18ZM11.45 10.16C10.18 10.41 9.35 11.34 9.41 12.46C9.43 12.79 9.47 12.97 9.61 13.26C10.23 14.56 12.04 15 13.32 14.18C14.99 13.09 14.55 10.77 12.55 10.22C12.37 10.17 11.62 10.13 11.45 10.16ZM20.19 10.18C19.21 10.37 18.47 11.02 18.25 11.88C18.15 12.27 18.2 12.82 18.36 13.18C18.91 14.43 20.57 14.97 21.86 14.32C23.56 13.46 23.58 11.28 21.89 10.44C21.45 10.22 21.25 10.17 20.74 10.16C20.51 10.16 20.26 10.16 20.19 10.18Z',
    warning: 'M11.01 0.93C6.23 1.15 2.1 4.3 0.59 8.86C0.08 10.43 -0.09 12.21 0.12 13.88C0.77 19.1 4.91 23.23 10.12 23.89C15.36 24.54 20.37 21.57 22.31 16.65C23.07 14.72 23.29 12.56 22.94 10.48C22.17 5.98 18.73 2.31 14.27 1.23C13.26 0.99 12.07 0.88 11.01 0.93ZM12.65 2.89C15.77 3.26 18.45 5.05 19.98 7.78C21.44 10.41 21.57 13.68 20.31 16.43C19.05 19.2 16.54 21.22 13.55 21.85C12.52 22.07 11.22 22.11 10.19 21.96C7.34 21.55 4.86 19.91 3.36 17.46C1.7 14.76 1.5 11.37 2.8 8.48C3.28 7.43 3.88 6.55 4.7 5.72C6.35 4.03 8.45 3.05 10.84 2.85C11.23 2.82 12.25 2.84 12.65 2.89ZM11.38 6.63C10.96 6.67 10.63 6.95 10.54 7.35C10.49 7.56 10.65 13.71 10.71 13.91C10.83 14.31 11.12 14.49 11.6 14.47C12.1 14.45 12.35 14.21 12.4 13.7C12.43 13.49 12.57 8.27 12.57 7.7C12.57 7.46 12.57 7.38 12.52 7.26C12.36 6.81 11.92 6.57 11.38 6.63ZM11.15 15.88C10.61 16.07 10.27 16.61 10.35 17.13C10.41 17.52 10.67 17.87 11.04 18.05C11.23 18.14 11.24 18.14 11.55 18.14C11.86 18.14 11.87 18.14 12.06 18.05C12.77 17.7 12.99 16.86 12.51 16.26C12.21 15.88 11.62 15.72 11.15 15.88Z',
    close: 'M3.09 2.71C2.69 2.81 2.33 3.1 2.15 3.48C1.93 3.94 1.93 4.42 2.15 4.87L2.26 5.09L5.82 8.65L9.38 12.21L5.87 15.71C3.64 17.95 2.32 19.27 2.25 19.38C1.84 19.96 1.91 20.81 2.41 21.31C2.88 21.78 3.73 21.88 4.29 21.53C4.34 21.49 6 19.87 7.96 17.91L11.53 14.36L15.06 17.89C17.58 20.41 18.63 21.45 18.74 21.52C19 21.68 19.21 21.74 19.56 21.74C19.9 21.74 20.01 21.71 20.3 21.56C21.04 21.18 21.3 20.13 20.83 19.42C20.76 19.31 19.64 18.17 17.2 15.74L13.68 12.21L17.23 8.65C20.54 5.34 20.8 5.07 20.88 4.91C21.02 4.63 21.06 4.48 21.06 4.16C21.06 3.84 20.99 3.6 20.82 3.34C20.42 2.7 19.54 2.48 18.84 2.84C18.67 2.93 18.41 3.19 15.1 6.5L11.52 10.06L7.96 6.5C4.47 3.01 4.38 2.93 4.2 2.84C4.1 2.79 3.95 2.73 3.86 2.71C3.68 2.66 3.29 2.67 3.09 2.71Z',
    airplay: 'M0.39 1.04C-0.83 1.23 -1.65 1.96 -1.91 3.08C-2 3.48 -2 3.57 -1.99 9.76L-1.98 15.81L-1.93 16.04C-1.66 17.19 -0.98 17.86 0.19 18.13C0.41 18.18 0.48 18.19 2.7 18.19L4.99 18.2L5.82 17.3L6.65 16.39L3.68 16.38C1.01 16.36 0.7 16.36 0.59 16.32C0.2 16.19 -0.03 15.91 -0.1 15.47C-0.13 15.25 -0.13 3.95 -0.1 3.73C-0.02 3.27 0.23 2.98 0.66 2.86C0.8 2.83 2 2.82 11.69 2.82C22.28 2.82 22.58 2.83 22.72 2.87C23.13 3 23.38 3.29 23.45 3.7C23.49 3.98 23.48 15.34 23.44 15.55C23.36 15.93 23.12 16.19 22.76 16.32C22.65 16.36 22.34 16.36 19.68 16.38L16.72 16.39L17.55 17.3L18.38 18.2L20.67 18.19L22.96 18.19L23.23 18.12C24.42 17.82 25.13 17.05 25.31 15.84C25.37 15.46 25.37 3.74 25.31 3.36C25.17 2.41 24.7 1.73 23.92 1.34C23.69 1.22 23.49 1.15 23.16 1.07L22.94 1.01L11.77 1.01C2.34 1.01 0.58 1.01 0.39 1.04ZM11.5 13.57C11.37 13.62 11.28 13.68 11.13 13.83C10.62 14.35 4.05 21.57 4.01 21.64C3.87 21.94 4 22.28 4.3 22.39C4.39 22.42 5.19 22.42 11.68 22.42C17.13 22.42 18.98 22.42 19.05 22.39C19.36 22.3 19.52 21.92 19.37 21.63C19.33 21.57 12.71 14.31 12.27 13.85C12.17 13.75 12.04 13.65 11.99 13.62C11.88 13.56 11.61 13.54 11.5 13.57Z',
    tv: 'M-0.43 0.5C-1.78 0.64 -2.64 1.41 -2.93 2.73L-2.98 2.96L-2.99 10.06C-3 16.28 -2.99 17.2 -2.96 17.45C-2.78 18.89 -1.83 19.79 -0.37 19.92C0.03 19.96 23.27 19.96 23.67 19.92C24.91 19.81 25.8 19.12 26.14 18.02C26.3 17.48 26.29 18.09 26.29 10.24C26.29 4.53 26.28 3.09 26.26 2.93C26.11 1.95 25.63 1.23 24.87 0.84C24.62 0.71 24.3 0.59 23.98 0.53C23.77 0.49 23.16 0.49 11.76 0.48C5.16 0.48 -0.33 0.49 -0.43 0.5ZM23.63 2.41C24.03 2.54 24.27 2.79 24.36 3.18C24.4 3.33 24.4 4.19 24.4 10.28L24.39 17.21L24.33 17.36C24.23 17.62 24.08 17.8 23.85 17.91C23.58 18.05 24.4 18.04 11.61 18.04L-0.24 18.04L-0.39 17.98C-0.76 17.85 -0.98 17.62 -1.08 17.25C-1.12 17.11 -1.14 3.69 -1.1 3.36C-1.04 2.86 -0.77 2.53 -0.35 2.41C-0.13 2.36 23.43 2.36 23.63 2.41ZM5.54 22.03C4.9 22.18 4.61 22.98 5 23.52C5.11 23.67 5.29 23.79 5.48 23.85C5.63 23.9 5.68 23.9 11.7 23.89L17.76 23.89L17.91 23.82C18.1 23.73 18.2 23.64 18.31 23.49C18.64 22.99 18.44 22.3 17.88 22.07L17.74 22.01L11.69 22.01C8.23 22.01 5.6 22.02 5.54 22.03Z'
  };
  // `point: 'forward'` turns the `down` chevron to point forward, as a folded section's chevron does (SF Symbols' chevron.forward). `label`: the glyph's name for a screen reader, as an image; without one, a screen reader skips it.
  function Glyph(props) {
    if (props.point != null && !(props.point === 'forward' && props.name === 'down')) throw new Error('Glyph turns only the `down` chevron, to point `forward`');
    var cls = cx('bae-glyph', props.point && 'bae-glyph-forward', props.className);
    if (props.set !== 'mac') return h(MSymbol, { name: props.name, size: props.size, className: cls, fill: 'currentColor', label: props.label });
    var set = SF, box = iconBox(props.size);
    var named = props.label ? { role: 'img', 'aria-label': props.label } : { 'aria-hidden': true };
    return h('svg', Object.assign({ viewBox: '0 0 24 24', className: cls, fill: 'currentColor', style: { width: box.width, height: box.height, overflow: 'visible' } }, named), h('path', { d: set[props.name] || set.ellipsis || '', fillRule: 'evenodd' }));
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
        h('div', { className: 'bae-seek-track', role: 'slider', 'aria-label': 'Position', 'aria-valuenow': Math.round(pos * 100), 'aria-disabled': props.off || undefined },
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
      h(Glyph, { set: 'mac', name: props.icon, size: props.size || 'large' }));
  }

  // A row's ⋯ button with the row's actions in its menu: a row offers its actions here, not as a button of each. `open` draws the menu under it, `items` its entries ("-" for a line between groups), `highlighted` the one under the pointer; `off` dims it while none of its actions can run. `boxed` fills it as a push button, for a page's header; `overlay` makes it a round button on a dark fill, for a place on a cover.
  function MacMoreMenu(props) {
    var btn = h(MacIconButton, { icon: 'ellipsis', size: 'medium', label: props.label || 'More', active: props.open, menu: true, off: props.off, boxed: props.boxed, overlay: props.overlay });
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
    // A button named by what it does, as the app's.
    var button = h('button', { type: 'button', className: 'bae-m-playpause', 'aria-label': props.playing ? 'Pause' : 'Play' },
      h(Glyph, { set: 'mac', name: props.playing ? 'pause' : 'play', size: 'large' }));
    if (!props.waiting) return button;
    // While the track waits on storage, the arc turns over the button: a progress indicator of its own beside it, which VoiceOver reads as Downloading the track… before the button, as the app's overlay.
    return h('span', { className: 'bae-m-playpause-wait' },
      h('svg', { className: 'bae-waitarc', width: 48, height: 48, viewBox: '0 0 48 48', role: 'progressbar', 'aria-label': 'Downloading the track…' }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })),
      button);
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
      h('div', { className: 'bae-m-volume', role: 'slider', 'aria-label': 'Volume', 'aria-valuenow': Math.round((props.volume != null ? Number(props.volume) : 0.7) * 100) }, h('div', { style: { width: ((props.volume != null ? Number(props.volume) : 0.7) * 100) + '%' } })));
  }

  function MacNowPlayingBar(props) {
    var none = !props.title;
    return h('div', { className: 'bae-m-npbar' },
      // The playing track: one button, its cover beside its title and artist, named as VoiceOver reads the merged track.
      h('div', { className: 'bae-m-np-left' }, none ? null : h('button', { type: 'button', className: 'bae-plainbtn bae-m-nptrack', 'aria-label': props.artist ? 'Now playing, ' + props.title + ' by ' + props.artist : props.title },
        h(Artwork, { title: props.album || props.title, size: 'size-bar-artwork', radius: 'radius-artwork', shadow: 'bar', missing: props.noCover }),
        h('div', { className: 'bae-m-np-text', 'aria-hidden': true }, h('div', { className: 'bae-m-np-title' }, props.title), h('div', { className: 'bae-m-np-sub' }, props.artist)))),
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
    if (props.state === 'attention') return h('span', { className: 'bae-m-iconbtn bae-m-sync-warn', title: 'Activity' }, h(Glyph, { set: 'mac', name: 'warning', size: 'large' }));
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
    return h('div', { className: 'bae-m-mainwin', style: { width: Number(props.width || 1200), height: Number(props.height || 760), flex: 'none' } }, macLights(),
      h(MacTitleBar, { section: props.section, search: props.search, searchFocused: props.searchFocused, sync: props.sync, avatar: props.avatar, avatarImage: props.avatarImage }),
      h('div', { className: 'bae-m-main-body' }, h('div', { className: 'bae-m-main-content' }, kids(props.children).filter(function (c) { var p = inner(c); return !(p && p.type === MacQueuePanel); })), kids(props.children).filter(function (c) { var p = inner(c); return p && p.type === MacQueuePanel; })),
      h(MacNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, noCover: props.trackNoCover, playing: props.playing, waiting: props.waiting, held: props.held, elapsed: props.elapsed, duration: props.duration, shuffle: props.shuffle, repeat: props.repeat, casting: props.casting, noVolume: props.noVolume, queueOpen: props.queueOpen }));
  }

  function MacLibraryHeading(props) {
    var collapsed = !!props.collapsed;
    return h('div', { className: cx('bae-m-libhead', collapsed && 'bae-m-libhead-collapsed') },
      // The view's name is a menu of the library's views.
      h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-plainbtn bae-m-libhead-title' }, props.view || 'Albums', h('span', { className: 'bae-m-libhead-chevron', 'aria-hidden': true }, h(Glyph, { set: 'mac', name: 'down', size: collapsed ? 'badge' : 'medium' }))),
      h('div', { className: 'bae-m-libhead-trail' }, props.children));
  }
  SF.down = 'M2.3 6.15C1.54 6.28 0.96 7.02 1.01 7.81C1.04 8.27 1.18 8.53 1.66 9.03C2.06 9.44 2.91 10.3 3.62 11.02C3.93 11.35 5.43 12.88 6.95 14.44C10.43 18 10.47 18.04 10.69 18.17C10.97 18.33 11.18 18.39 11.54 18.39C11.99 18.38 12.3 18.27 12.64 17.98C12.73 17.9 14.33 16.27 16.19 14.36C18.05 12.45 19.8 10.67 20.06 10.4C21.88 8.55 21.86 8.57 21.98 8.23C22.05 8.03 22.07 7.57 22.02 7.35C21.88 6.81 21.5 6.39 20.97 6.2C20.81 6.15 20.75 6.14 20.5 6.14C19.94 6.14 19.66 6.29 19.02 6.94C18.8 7.17 18.41 7.57 18.16 7.82C17.92 8.08 17.52 8.49 17.27 8.74C17.03 8.98 15.65 10.41 14.19 11.91C12.74 13.4 11.54 14.62 11.54 14.62C11.53 14.62 4.09 7 3.85 6.74C3.4 6.26 2.87 6.06 2.3 6.15Z';
  SF.sidebar = 'M2.55 1.31C1.18 1.45 0.21 1.84 -0.55 2.57C-1.23 3.21 -1.64 3.99 -1.86 5.01C-1.99 5.65 -1.99 5.35 -1.99 12.38C-1.99 19.59 -2 19.06 -1.84 19.8C-1.41 21.73 -0.05 22.98 1.98 23.31C2.54 23.4 2.68 23.4 12.23 23.39C22.61 23.39 21.7 23.4 22.42 23.23C24.27 22.77 25.47 21.39 25.76 19.35C25.84 18.81 25.85 18.12 25.85 12.35C25.85 6.6 25.84 5.89 25.76 5.36C25.46 3.27 24.26 1.9 22.36 1.45C21.72 1.3 22.52 1.31 12.12 1.31C6.91 1.31 2.6 1.31 2.55 1.31ZM13.8 12.34L13.8 20.77L8.28 20.77L2.76 20.76L2.55 20.71C1.38 20.41 0.75 19.66 0.62 18.4C0.59 18.06 0.59 6.64 0.62 6.29C0.76 4.86 1.62 4.02 3.05 3.93C3.21 3.91 5.69 3.91 8.56 3.91L13.79 3.91L13.8 12.34ZM21.12 3.96C22.34 4.18 23.06 4.95 23.2 6.17C23.22 6.34 23.23 8.21 23.22 12.51L23.21 18.61L23.16 18.83C22.89 19.89 22.31 20.46 21.25 20.71C21.08 20.76 20.85 20.76 18.64 20.77L16.23 20.78L16.23 12.35C16.23 7.72 16.23 3.93 16.24 3.91C16.25 3.9 17.3 3.9 18.58 3.91C20.59 3.91 20.92 3.92 21.12 3.96ZM18.35 6.56C18.2 6.6 17.99 6.74 17.91 6.84C17.77 7.02 17.74 7.13 17.74 7.34C17.74 7.58 17.81 7.76 17.98 7.92C18.26 8.17 18.34 8.18 19.86 8.17L21.06 8.16L21.23 8.09C21.41 7.99 21.58 7.84 21.66 7.68C21.75 7.49 21.74 7.16 21.64 6.98C21.55 6.81 21.39 6.67 21.2 6.59C21.06 6.54 21.05 6.54 19.77 6.53C18.7 6.53 18.47 6.53 18.35 6.56ZM18.35 9.79C17.97 9.89 17.74 10.18 17.74 10.56C17.74 10.88 17.89 11.12 18.18 11.28L18.34 11.36L19.73 11.36C21.3 11.36 21.22 11.37 21.47 11.14C21.89 10.75 21.78 10.08 21.24 9.84L21.09 9.76L19.77 9.76C18.79 9.75 18.44 9.76 18.35 9.79ZM18.32 13.01C17.66 13.22 17.52 14.05 18.09 14.44C18.33 14.6 18.35 14.6 19.79 14.59L21.09 14.59L21.23 14.52C21.62 14.34 21.82 13.92 21.69 13.53C21.63 13.36 21.44 13.15 21.26 13.06L21.11 12.99L19.77 12.98C18.67 12.98 18.42 12.98 18.32 13.01Z';

  // As many columns as the width holds tiles of 200 or more, as the app fits them.
  function MacAlbumGrid(props) {
    return h('div', { className: 'bae-m-grid' }, props.children);
  }

  // `unplayable`: only another device holds the album's files, so its cover and words show dimmed; the tile still opens the album. With no `artist`, the tile has no credit line, as under an artist's group when that artist is the album's only credit.
  function MacAlbumTile(props) {
    return h('div', { className: cx('bae-m-tile', props.selected && 'bae-m-tile-selected', props.unplayable && 'bae-tile-unplayable') },
      h('div', { className: 'bae-m-tile-art' },
        h(Artwork, { title: props.title, missing: props.noCover, radius: 'radius-cover', shadow: 'tile', ring: props.open, dimmed: props.unplayable }),
        props.hover ? h('span', { className: 'bae-m-tile-menu' }, h(MacMoreMenu, { overlay: true, label: 'Album options' })) : null),
      h('div', { className: 'bae-m-tile-title' }, props.title),
      props.artist ? h('div', { className: 'bae-m-tile-artist' }, props.artist) : null,
      h('div', { className: 'bae-m-tile-year' }, props.year || ' '));
  }

  // A track row's lead: the track's number; under the pointer (`hover`) a Play button, or Pause on the current track while it plays; on the current track otherwise the speaker, which a screen reader names Now playing.
  function MacTrackLead(props) {
    var pause = props.current && props.playing;
    return h('span', { className: 'bae-m-track-lead' },
      props.hover ? h('button', { type: 'button', className: 'bae-plainbtn', 'aria-label': pause ? 'Pause' : 'Play' }, h(Glyph, { set: 'mac', name: pause ? 'pause' : 'play', size: 'medium' }))
        : props.current ? h(Glyph, { set: 'mac', name: 'speaker', size: 'small', className: 'bae-accent', label: 'Now playing' })
        : h('span', { className: 'bae-m-track-pos' }, props.position));
  }

  // One track of an album's list. `picked`: among the tracks picked together, on the soft accent. `rating`: the row of an album's list, which keeps a slot for the track's stars after the row (0 leaves the slot empty, as the app does for a track nobody rated). `ratingSystem`: the library's system for tracks; `heart` fills the slot with a heart when the stars are three or more, `none` draws no slot.
  function MacTrackRow(props) {
    var row = h('div', { className: cx('bae-m-track', props.picked ? 'bae-m-track-picked' : props.hover && 'bae-m-track-hover', props.artist && 'bae-m-track-two') },
      h(MacTrackLead, { position: props.position, hover: props.hover, current: props.current, playing: props.playing }),
      h('div', { className: 'bae-m-track-text' },
        h('div', { className: cx('bae-m-track-title', props.current && 'bae-accent') }, props.title),
        props.artist ? h('div', { className: 'bae-m-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-m-track-time' }, props.duration));
    // Every album list keeps a slot for the track's stars after the row, empty when you gave it none, so every row's duration lines up.
    if (props.ratingSystem === 'none') return row;
    var n = Number(props.rating || 0), heart = props.ratingSystem === 'heart', shown = heart ? n >= 3 : n > 0;
    return h('div', { className: 'bae-m-track-rated' }, h('div', { className: 'bae-m-track-rated-row' }, row),
      h('span', { className: cx('bae-m-track-stars', !shown && 'bae-m-track-stars-none'), 'aria-hidden': !shown || null }, heart ? h(Heart, { on: true, size: 10 }) : h(Stars, { value: n, size: 10 })));
  }

  function MacEyebrow(props) { return h('div', { className: 'bae-m-eyebrow' }, props.children); }

  // `options`: the page's ⋯ button at the right with these actions in its menu, as an album's options button; `optionsOpen` draws the menu open, `highlighted` the item under the pointer.
  function MacDetailHeader(props) {
    return h('div', { className: 'bae-m-dethead' },
      h(Artwork, { title: props.name, size: 72, radius: 'radius-cover', missing: props.noImage }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-m-dethead-name' }, props.name), h('div', { className: 'bae-m-dethead-count' }, props.count)),
      props.options ? h(MacMoreMenu, { label: props.optionsLabel || 'Options', items: props.options, open: props.optionsOpen, highlighted: props.highlighted }) : null);
  }

  // A playlist page's head, laid out as the album page's: the picture beside a column of the name (up to two lines), the owner on a line of their own, the facts line (count and length), then Play and the ⋯ under the text, at its start. The ⋯ menu offers Shuffle to everyone, then the owner's `options`; `optionsOpen` draws it open, `highlighted` the item under the pointer. The picture is as tall as the text column. `sharing`: on the person's own playlist, who sees it, as a lock (Only me) or people (Everyone) glyph at the facts line's end; a playlist someone else shared shows nothing about sharing.
  function MacPlaylistHeader(props) {
    var shared = props.sharing ? h('span', { className: 'bae-m-plpage-sharing', role: 'img', 'aria-label': props.sharing, title: props.sharing }, h(Glyph, { set: 'mac', name: props.sharing === 'Everyone' ? 'people' : 'lock', size: 'small' })) : null;
    return h('div', { className: 'bae-m-plpage-head' },
      h(PlaylistPicture, { covers: props.covers, size: 103, radius: 'radius-cover' }),
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
    return h('button', { type: 'button', className: 'bae-m-close', 'aria-label': props.label || 'Close', title: props.label || 'Close' }, h(Glyph, { set: 'mac', name: 'close', size: 'medium' }));
  }

  // The choice of an album's release, one segment for each of `releases` (comma-separated), `selected` the one showing; an album of one release has no choice to draw.
  function MacReleasePicker(props) {
    var releases = list(props.releases);
    if (releases.length < 2) return null;
    return h(MacSegmentedControl, { options: releases.join(','), selected: props.selected || releases[0] });
  }

  // The open album's head: the title, the artist, the year, each on its own line, the release picker (`releases`, `release`), the edition line (`edition`) and the copy line (`copy`), the line saying why this Mac can't play it (`unplayable`), Play and the album's ⋯ (`options`, `optionsOpen`, `highlighted`, a boxed MacMoreMenu). After the ⋯ comes the person's own rating, read-only (`rating`, "4 ★"): nothing while they have not rated the album. The rest of the album's actions, the rating included, are items of the ⋯ menu.
  // The album's read-only rating after its ⋯ or year: "4 ★" under Stars, one filled heart under Heart at three stars or more, nothing under None or while the person has not rated it (or not hearted it).
  function albumMark(rating, system, size, className) {
    var n = Number(rating || 0);
    if (system === 'none' || !n || (system === 'heart' && n < 3)) return null;
    var mark = system === 'heart' ? h(Heart, { on: true, size: size }) : h(Stars, { value: n, size: size, number: true });
    return className ? h('span', { className: className }, mark) : mark;
  }

  function MacAlbumHeader(props) {
    var picker = h(MacReleasePicker, { releases: props.releases, selected: props.release });
    return h('div', { className: 'bae-m-albumhead' },
      h('h2', { className: 'bae-m-albumpanel-title' }, props.title),
      h('div', { className: 'bae-m-albumpanel-sub' }, h('span', { className: 'bae-muted' }, props.artist), props.year ? [h('span', { key: 'd', className: 'bae-m-tertiary' }, '·'), h('span', { key: 'y', className: 'bae-m-tertiary' }, props.year)] : null),
      list(props.releases).length > 1 ? h('div', { className: 'bae-m-albumpanel-releases' }, picker) : null,
      props.edition || props.copy ? h('div', { className: 'bae-m-albumpanel-facts' }, props.edition ? h('div', null, props.edition) : null, props.copy ? h('div', null, props.copy) : null) : null,
      // `unplayable`: where the files are, when only another device holds them; Play gives way to the words Not on this device, whose help tag names the device (`unplayableHover` draws it showing).
      h('div', { className: 'bae-m-albumpanel-actions' }, props.unplayable ? h(MacHelpTag, { text: props.unplayableHover ? props.unplayable : null }, h('span', { className: 'bae-m-albumpanel-unplayable' }, 'Not on this device')) : h(MacButton, { kind: 'primary' }, 'Play'), h(MacMoreMenu, { boxed: true, label: 'Album options', items: props.options, open: props.optionsOpen, highlighted: props.highlighted }),
        albumMark(props.rating, props.ratingSystem, 13, 'bae-m-albumpanel-rating')));
  }

  // A help tag: the short words macOS shows under the pointer after it rests on a control (SwiftUI's `.help`), drawn under the control it describes; with no `text`, only the control.
  function MacHelpTag(props) {
    return h('span', { className: 'bae-m-helptag-anchor' }, props.children, props.text ? h('span', { role: 'tooltip', className: 'bae-m-helptag' }, props.text) : null);
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

  // The album opened in the grid: a panel under the album's row, its 340pt cover beside its header (`MacAlbumHeader`, which takes the same props), then its tracks, credits and works as children, then the release's audio line (`audio`).
  function MacAlbumPanel(props) {
    return h('section', { className: 'bae-m-albumpanel' },
      h('span', { className: 'bae-m-albumpanel-close' }, h(MacCloseButton)),
      h(Artwork, { title: props.title, size: 340, radius: 'radius-cover', missing: props.noCover, className: 'bae-m-albumpanel-cover' }),
      h('div', { className: 'bae-m-albumpanel-body' },
        h(MacAlbumHeader, { title: props.title, artist: props.artist, year: props.year, releases: props.releases, release: props.release, edition: props.edition, copy: props.copy, unplayable: props.unplayable, unplayableHover: props.unplayableHover, options: props.options, optionsOpen: props.optionsOpen, highlighted: props.highlighted, rating: props.rating, ratingSystem: props.ratingSystem }),
        h('div', { className: 'bae-m-albumpanel-tracks bae-m-albumpanel-tracks-rated' }, props.children),
        props.audio ? h('div', { className: 'bae-m-albumpanel-audio' }, props.audio) : null));
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

  // `unplayable`: dimmed as `MacAlbumTile` draws it.
  function MacArtistAlbumTile(props) {
    return h('div', { className: cx('bae-m-atile', props.unplayable && 'bae-tile-unplayable') }, h(Artwork, { title: props.title, radius: 'radius-artwork', missing: props.noCover, dimmed: props.unplayable }),
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
      // The field is a menu of the fields; the arrow, named for the direction, flips it.
      h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-plainbtn bae-m-sortpill-field' }, props.field, h(Glyph, { set: 'mac', name: 'down', size: 'badge', className: 'bae-muted' })),
      h('button', { type: 'button', className: 'bae-plainbtn bae-muted', 'aria-label': down ? 'Descending' : 'Ascending' }, arrow),
      props.removable ? h('button', { type: 'button', className: 'bae-plainbtn bae-muted', 'aria-label': 'Remove sort criterion' }, h(Glyph, { set: 'mac', name: 'close', size: 'badge' })) : null);
  }

  // The sort's criteria (`criteria`: "Field:down,Field:up"), each a `MacSortPill`; then + to add one, while a field of the list (`fields`, the albums' Title, Artist, Year and Date added unless given) is still unused.
  function MacSortCriteria(props) {
    var items = String(props.criteria || 'Date added:down').split(',').map(function (c) { var p = c.trim().split(':'); return { field: p[0], dir: p[1] || 'up' }; });
    return h('div', { className: 'bae-m-sortrow' },
      items.map(function (it, i) { return h(MacSortPill, { key: i, field: it.field, direction: it.dir, removable: items.length > 1 }); }),
      list(props.fields || 'Title, Artist, Year, Date added').every(function (f) { return items.some(function (it) { return it.field === f; }); }) ? null : h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-plainbtn bae-m-sortadd', 'aria-label': 'Add sort criterion' }, h(Glyph, { set: 'mac', name: 'add', size: 'medium' })));
  }
  SF.add = 'M11.54 2.99C11.07 3.1 10.65 3.49 10.47 3.96L10.41 4.11L10.41 7.55L10.4 11L6.82 11L3.24 11.01L3.02 11.09C1.62 11.59 1.69 13.43 3.12 13.87C3.25 13.91 3.57 13.91 6.83 13.92L10.4 13.93L10.4 17.3C10.4 20.57 10.4 20.67 10.45 20.86C10.52 21.13 10.61 21.28 10.84 21.52C11.22 21.9 11.71 22.05 12.24 21.94C12.8 21.82 13.27 21.35 13.39 20.81C13.42 20.66 13.43 20.11 13.43 17.28L13.43 13.93L17 13.92C20.97 13.91 20.64 13.93 21.02 13.73C21.23 13.63 21.5 13.38 21.61 13.19C22.08 12.38 21.71 11.41 20.8 11.09L20.59 11.01L17.01 11L13.43 11L13.42 7.57L13.41 4.14L13.36 3.97C13.11 3.24 12.3 2.8 11.54 2.99Z';
  // Who sees a playlist of the person's own: a lock for Only me, two people for Everyone.
  SF.lock = 'M11.46 1.34C8.66 1.48 6.56 3.28 6.02 6.01C5.88 6.69 5.87 6.84 5.86 8.67L5.84 10.37L5.7 10.4C4.68 10.6 4.1 11.18 3.87 12.23L3.81 12.49L3.81 16.88C3.81 21.06 3.82 21.27 3.86 21.51C4.06 22.58 4.61 23.16 5.66 23.41L5.89 23.46L11.62 23.47C15.77 23.48 17.43 23.47 17.61 23.45C18.84 23.31 19.52 22.67 19.75 21.45C19.81 21.07 19.81 12.74 19.74 12.35C19.59 11.5 19.22 10.95 18.59 10.63C18.37 10.53 17.98 10.4 17.87 10.4C17.75 10.4 17.76 10.44 17.74 8.66C17.73 6.85 17.72 6.72 17.59 6.04C17.05 3.31 15.03 1.54 12.2 1.35C11.85 1.33 11.79 1.32 11.46 1.34ZM12.16 3.05C13.93 3.21 15.22 4.26 15.7 5.94C15.87 6.54 15.91 7.04 15.9 8.91L15.89 10.34L11.8 10.34L7.71 10.34L7.71 8.65C7.71 6.89 7.72 6.77 7.83 6.24C8.16 4.62 9.32 3.45 10.89 3.12C11.36 3.03 11.73 3.01 12.16 3.05Z';
  SF.people = 'M17.81 0.75C15.27 0.99 13.33 3.31 13.43 6C13.57 9.66 17.16 12.01 20.24 10.46C22.32 9.42 23.43 6.98 22.91 4.6C22.38 2.2 20.18 0.53 17.81 0.75ZM3.87 2.36C1.76 2.66 0.2 4.52 0.2 6.74C0.2 9.55 2.64 11.68 5.22 11.12C6.64 10.81 7.84 9.7 8.32 8.23C9.2 5.62 7.71 2.9 5.13 2.4C4.78 2.33 4.21 2.31 3.87 2.36ZM3.85 13.02C1.41 13.14 -0.8 14.09 -2.39 15.7C-3.45 16.76 -4.15 18.07 -4.36 19.38C-4.42 19.75 -4.44 20.3 -4.39 20.55C-4.26 21.27 -3.73 21.67 -2.77 21.77C-2.61 21.79 -0.9 21.8 2.16 21.8L6.85 21.8L6.77 21.65C6.41 20.92 6.38 19.86 6.69 18.73C7.07 17.31 7.93 15.98 9.29 14.71L9.58 14.43L9.36 14.3C8.33 13.69 7.06 13.27 5.76 13.1C5.42 13.05 4.48 12.99 4.3 13C4.27 13 4.06 13.02 3.85 13.02ZM17.46 13.07C17.41 13.07 17.22 13.09 17.04 13.1C15.81 13.2 14.48 13.53 13.36 14C10.44 15.24 8.31 17.66 8.12 19.96C8.03 21.15 8.58 21.66 10.1 21.77C10.36 21.8 12.67 21.8 18.49 21.79C25.8 21.78 26.53 21.78 26.74 21.74C27.69 21.57 28.11 21.29 28.29 20.69C28.33 20.57 28.34 20.46 28.34 20.2C28.33 18.29 26.78 16.09 24.47 14.71C22.98 13.82 21.33 13.29 19.46 13.1C19.19 13.07 17.66 13.04 17.46 13.07Z';
  // The filled circle with an × that removes one entry of a list (xmark.circle.fill).
  SF.removefill = 'M11.01 0.93C7.34 1.1 3.98 3.02 1.94 6.1C-0.02 9.05 -0.5 12.73 0.62 16.1C1.77 19.51 4.49 22.23 7.9 23.38C10.48 24.24 13.27 24.16 15.81 23.16C16.27 22.98 17.22 22.51 17.63 22.24C19.35 21.16 20.72 19.73 21.69 17.97C22.98 15.64 23.4 12.87 22.89 10.22C21.8 4.63 16.71 0.65 11.01 0.93ZM7.75 7.57C8.04 7.63 8.02 7.61 9.82 9.41C10.77 10.35 11.55 11.12 11.57 11.12C11.59 11.12 12.38 10.35 13.32 9.41C14.69 8.05 15.06 7.69 15.16 7.65C15.64 7.44 16.18 7.63 16.4 8.1C16.45 8.22 16.46 8.27 16.46 8.49C16.46 8.89 16.57 8.77 14.61 10.74L12.9 12.46L14.62 14.19C16.59 16.16 16.46 16.01 16.46 16.44C16.46 16.74 16.4 16.91 16.2 17.11C16.01 17.3 15.84 17.36 15.55 17.36C15.13 17.36 15.28 17.48 13.31 15.51L11.57 13.79L9.86 15.5C7.89 17.46 8.01 17.36 7.59 17.36C7.39 17.36 7.34 17.35 7.22 17.3C6.73 17.06 6.52 16.48 6.77 15.99C6.81 15.9 7.25 15.45 8.54 14.17L10.25 12.46L8.53 10.74C7.21 9.42 6.8 8.98 6.75 8.9C6.5 8.35 6.8 7.71 7.38 7.59C7.58 7.54 7.59 7.54 7.75 7.57Z';

  function MacCheckbox(props) {
    return h('label', { className: 'bae-m-checkbox', role: 'checkbox', 'aria-checked': !!props.on }, h('span', { className: cx('bae-m-checkbox-box', props.on && 'bae-m-checkbox-on') }, props.on ? h(CheckGlyph, { size: 'chip' }) : null), props.label);
  }

  // Heads an artist's albums when the grid is grouped by artist: the artist's picture, as the artist list draws it, beside the name over the album count. `noImage`: the artist has no picture.
  function MacArtistGroupHeading(props) {
    return h('div', { className: 'bae-m-groupheading' },
      h(Artwork, { title: props.name, size: 'size-row-artwork', radius: 'radius-artwork', missing: props.noImage }),
      h('div', { className: 'bae-m-groupheading-text' }, h('div', { className: 'bae-m-groupheading-name' }, props.name), h('div', { className: 'bae-m-groupheading-count' }, props.count)));
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
    // `label`: the item's name for a screen reader when it shows a picture in place of words, such as a row of stars.
    return h('div', { className: cx('bae-a-menu-item', props.selected && 'bae-a-menu-item-on', props.pressed && 'bae-a-menu-item-pressed'), role: 'menuitem', 'aria-label': props.label },
      h('span', null, props.children),
      props.check != null ? h('span', { className: 'bae-a-menu-check', style: { opacity: props.check ? 1 : 0 } }, h(Material, { name: 'check', size: 'large' })) : null);
  }
  MATERIAL.check = 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z';

  // ---- Android library ----

  function AndroidPlayPause(props) {
    // The button is the glyph and 8dp around it, which the waiting arc rings.
    var icon = props.size || 'large', box = 'calc(' + iconSize(icon, true) + ' + 16px)';
    return h('button', { type: 'button', className: 'bae-a-playpause', style: { width: box, height: box }, 'aria-label': props.playing || props.waiting ? 'Pause' : 'Play' },
      props.waiting ? h('svg', { className: 'bae-waitarc', style: { width: box, height: box }, viewBox: '0 0 48 48', 'aria-hidden': true }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })) : null,
      h(Glyph, { name: props.playing || props.waiting ? 'pause' : 'play', size: icon }));
  }

  // The playing track on Android: its cover beside its title and artist.
  function AndroidNowPlayingTrack(props) {
    // One button that opens the full player, named as TalkBack reads the merged track.
    return h('button', { type: 'button', className: 'bae-a-np-info', 'aria-label': props.artist ? 'Now playing, ' + props.title + ' by ' + props.artist : props.title }, h(Artwork, { title: props.album || props.title, size: 'size-android-bar-artwork', radius: 'radius-artwork', missing: props.noCover }),
      h('div', { className: 'bae-a-np-text', 'aria-hidden': true }, h('div', { className: 'bae-a-np-title' }, props.title), h('div', { className: 'bae-a-np-sub' }, props.artist)));
  }

  // Previous, play and pause, next. `large` is the full player's: bigger buttons, spread across the sheet; `waiting` draws the arc around play while the track waits on storage.
  function AndroidTransport(props) {
    var large = !!props.large;
    return h('div', { className: cx('bae-a-transport', large && 'bae-a-transport-large') },
      h(AndroidIconButton, { box: large ? 56 : 40, size: large ? 'androidTransport' : null, icon: 'previous', label: 'Previous' }),
      h(AndroidPlayPause, { playing: props.playing, waiting: props.waiting, size: large ? 'androidTransportPlay' : null }),
      h(AndroidIconButton, { box: large ? 56 : 40, size: large ? 'androidTransport' : null, icon: 'next', label: 'Next' }));
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

  // The library header while searching: Close search, and the search field with what was typed (`search`) and its Clear search button, or the prompt.
  function AndroidSearchBar(props) {
    return h('header', { className: 'bae-a-searchbar' }, h(AndroidIconButton, { icon: 'back', label: 'Close search' }),
      h('div', { className: 'bae-a-searchfield' }, h('span', { className: props.search ? null : 'bae-a-search-prompt' }, props.search || 'Search albums, tracks, composers, and works'), props.search ? h(AndroidInlineIconButton, { icon: 'close', label: 'Clear search' }) : null));
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
      h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-a-libhead-title' }, h('span', null, props.view || 'Albums'), h(Glyph, { name: 'down', size: collapsed ? 'small' : 'medium', className: 'bae-a-libhead-chevron' })),
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

  // A page about one thing under its back bar. The page's content carries its own padding, as each page's list does in the app (the album's in `AndroidAlbumDetailContent`).
  function AndroidDetailScreen(props) {
    var split = splitOverlays(props.children);
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      h(AndroidTopBar, { back: true, backLabel: props.back ? 'Back to ' + props.back : 'Back to library', more: props.menu ? (props.menuLabel || 'Album options') : null }),
      h('main', { className: 'bae-a-libbody' }, split.content),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, noCover: props.trackNoCover, playing: props.playing, waiting: props.waiting, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat }),
      split.over);
  }

  // The album page below its bar: its blocks (the header, the release's controls, the tracks, the credits) in a 16dp column, 8dp apart.
  function AndroidAlbumDetailContent(props) {
    return h('div', { className: 'bae-a-albumcontent' }, props.children);
  }

  function AndroidAlbumGrid(props) {
    return h('div', { className: 'bae-a-grid' }, props.children);
  }

  // An album's tile: one button that opens the album, named by its title and line, as TalkBack reads the merged tile. `unplayable`: only another device holds the album's files, so its cover and words show dimmed; the tile still opens the album.
  function AndroidAlbumTile(props) {
    return h('button', { type: 'button', className: cx('bae-plainbtn', 'bae-a-tile', props.unplayable && 'bae-tile-unplayable') },
      h(Artwork, { title: props.title, missing: props.noCover, radius: 'radius-cover', dimmed: props.unplayable }),
      h('div', { className: 'bae-a-tile-title' }, props.title),
      h('div', { className: 'bae-a-tile-line' }, props.line));
  }

  function AndroidArtistGroupHeading(props) {
    if (props.pinned) return h('div', { className: 'bae-a-artisthead-pinned' }, h(Artwork, { title: props.name, size: 32, round: true, missing: props.noImage }), h('div', { className: 'bae-a-artisthead-name' }, props.name));
    return h('div', { className: 'bae-a-artisthead' }, h(Artwork, { title: props.name, size: 'size-android-row-artwork', round: true, missing: props.noImage }),
      h('div', null, h('div', { className: 'bae-a-artisthead-name' }, props.name), h('div', { className: 'bae-a-artisthead-count' }, props.count)));
  }

  // A track row's lead on Android: the track's number, or on the current track Play, or the playing mark while it plays. A screen reader names the playing mark Now playing and the Play mark Paused.
  function AndroidTrackLead(props) {
    return h('span', { className: 'bae-a-track-lead' }, props.current ? h(Glyph, { name: props.playing ? 'nowplaying' : 'play', size: 'large', className: 'bae-accent', label: props.playing ? 'Now playing' : 'Paused' }) : h('span', { className: 'bae-a-track-pos' }, props.position));
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
    var mark = albumMark(props.rating, props.ratingSystem, 12);
    return h('div', { className: 'bae-a-albumhead' }, h(Artwork, { title: props.title, size: 150, radius: 'radius-cover', missing: props.noCover }),
      h('div', { className: 'bae-a-albumhead-text' }, h('div', { className: 'bae-a-albumhead-title' }, props.title), h('div', { className: 'bae-a-albumhead-artist' }, props.artist), props.year || mark ? h('div', { className: 'bae-a-albumhead-fact' }, props.year, props.year && mark ? ' · ' : null, mark) : null,
        props.edition ? h('div', { className: 'bae-a-albumhead-fact bae-a-albumhead-release bae-a-albumhead-gap' }, props.edition) : null, props.copy ? h('div', { className: 'bae-a-albumhead-fact bae-a-albumhead-release' }, props.copy) : null,
        // `unplayable`: only another device holds the files, so the words Not on this device stand where Play stood, and the info button opens a sheet that names the device.
        props.unplayable ? h('div', { className: 'bae-a-albumhead-actions' }, h('span', { className: 'bae-a-albumhead-unplayable' }, 'Not on this device'), h(AndroidIconButton, { icon: 'info', label: 'Show where it is', size: 'medium', tone: 'muted' })) : null,
        props.children ? h('div', { className: 'bae-a-albumhead-actions' }, props.children) : null));
  }

  // The album page's pin control for one release while a pin is under way or has failed, as AndroidDownloadControlContainer draws it: Queued to pin, with Cancel; Pinning with its bar and bytes and Cancel pinning; Could not pin with Details and Retry, and Cancel. Pin and Unpin are items of the album's ⋮ menu. Nothing says how many wait ahead or what holds the queue back.
  function AndroidPinControl(props) {
    var s = props.state || 'queued';
    function btn(text) { return h(AndroidButton, { kind: 'outlined' }, text); }
    var line = function (t) { return h('span', { className: 'bae-a-fineline' }, t); };
    var body =
      s === 'downloading' ? [h(AndroidProgress, { key: 'p', name: 'Pinning', value: Number(props.progress || 0.4) }), h('span', { key: 'b', className: 'bae-a-fine' }, 'Pinning · ' + (props.bytes || '120 MB of 300 MB')), h('div', { key: 'c' }, btn('Cancel pinning'))] :
      s === 'failed' ? [h(AndroidNotice, { key: 'e', quiet: true, tone: 'danger', title: 'Could not pin', secondary: 'Details', action: 'Retry' }), h('div', { key: 'r' }, btn('Cancel'))] :
      h('div', { className: 'bae-a-dlrow' }, line('Queued to pin'), btn('Cancel'));
    return h('div', { className: 'bae-a-dl' }, body);
  }

  // The album page's upload line, as ReleaseUploadLine draws it: the phase core words, Queued to upload while the files wait; a missing original (`missing`) as a quiet warning notice naming it.
  function AndroidUploadLine(props) {
    return h('div', { className: 'bae-a-dl' }, h('span', { className: 'bae-a-fineline' }, props.phase || 'Queued to upload'),
      props.missing ? h(AndroidNotice, { quiet: true, tone: 'warning', title: props.missing + ' is missing', subtitle: 'Restore it to finish uploading.' }) : null);
  }
