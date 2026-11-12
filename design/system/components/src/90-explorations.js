  // ==== part: explore ====
  // ---- Explorations: parts for screens bae doesn't have yet ----
  // 1–5 stars, filled up to value; empty draws all five faint. who adds a line of who else rated it.
  function Stars(props) {
    var v = Number(props.value || 0), s = Number(props.size || 14);
    var star = 'M12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7L12 17.3 5.8 20.9l1.6-7L2 9.2l7.1-.6z';
    return h('span', { className: 'bae-stars', role: 'img', 'aria-label': v ? v + ' stars' : 'Not rated' },
      [1, 2, 3, 4, 5].map(function (i) { return h('svg', { key: i, width: s, height: s, viewBox: '0 0 24 24', 'aria-hidden': true }, h('path', { d: star, fill: i <= v ? 'var(--accent)' : 'none', stroke: i <= v ? 'var(--accent)' : 'var(--hairline-strong)', strokeWidth: 1.5, strokeLinejoin: 'round' })); }),
      props.who ? h('span', { className: 'bae-muted', style: { marginLeft: 6, fontSize: '0.85em' } }, props.who) : null);
  }
  // A person's picture: initials on a tint when there's no image.
  function Avatar(props) {
    var s = Number(props.size || 32), name = String(props.name || '?');
    var ini = name.split(/\s+/).map(function (p) { return p[0]; }).join('').slice(0, 2).toUpperCase();
    return props.image ? h(Artwork, { title: props.image, size: s, round: true }) : h('span', { className: 'bae-avatar', style: { width: s, height: s, fontSize: s * 0.4 } }, ini);
  }
  // A row of a history or playlist list: when or position, cover, title, artist · album, who, length.
  function MacListRow(props) {
    return h('div', { className: cx('bae-m-lrow', props.selected && 'bae-m-lrow-selected') },
      props.lead ? h('span', { className: 'bae-muted bae-num', style: { width: 56, flex: 'none', fontSize: 11.5 } }, props.lead) : null,
      h(Artwork, { title: props.album || props.title, size: 32, radius: 4 }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-ellipsis', style: { fontWeight: 500 } }, props.title), h('div', { className: 'bae-muted bae-ellipsis', style: { fontSize: 11.5 } }, props.sub)),
      props.who ? h('span', { style: { display: 'flex', alignItems: 'center', gap: 6 }, className: 'bae-muted' }, h(Avatar, { name: props.who, size: 18 }), h('span', { style: { fontSize: 11.5 } }, props.who)) : null,
      props.stars ? h(Stars, { value: props.stars, size: 11 }) : null,
      h('span', { className: 'bae-muted bae-num', style: { width: 40, textAlign: 'right', fontSize: 11.5 } }, props.duration));
  }
  function AndroidListRow(props) {
    return h('div', { className: cx('bae-a-lrow', props.selected && 'bae-a-lrow-selected') },
      h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-a-librow-title' }, props.title), h('div', { className: 'bae-a-librow-sub' }, props.sub)),
      props.who ? h(Avatar, { name: props.who, size: 24 }) : null,
      props.trailing ? h('span', { className: 'bae-a-fine bae-muted' }, props.trailing) : null);
  }
