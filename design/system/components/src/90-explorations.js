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
  // A person's small picture, or initials, beside their name (`name`), as a history row ends in whose play it was. `strong` is a playlist header's owner line: the name in the header's semibold, at the size of the text around it.
  function MacPersonTag(props) {
    return h('span', { className: cx('bae-m-persontag', props.strong && 'bae-m-persontag-strong') },
      h(Avatar, { name: props.name, size: props.strong ? 16 : 18 }),
      h('span', { className: props.strong ? null : 'bae-m-persontag-name' }, props.name));
  }
  // A row of a history or playlist list: when or position, cover, title, artist · album, who, length.
  function MacListRow(props) {
    // `wide`: more room at the end, for a row that ends in a person (History). `cover`: the cover's size, 32pt unless given (40 on a playlist's page in the side pane). While the owner drags a playlist's row, `lifted` is the row left in its place, faded, and `dragging` the copy under the pointer, raised over the rows. `hanging`: the leading column hangs in the margin left of the row, right-aligned and `leadWidth` wide (20pt for a number; History's times need more), so the covers start at the list's edge, and the credit line sits 3pt under the title (a playlist's page in the side pane).
    var lw = props.hanging ? Number(props.leadWidth || 20) : null;
    return h('div', { style: lw != null ? { marginLeft: -(lw + 10) } : null, className: cx('bae-m-lrow', props.wide && 'bae-m-lrow-wide', props.hanging && 'bae-m-lrow-hanging', props.lifted && 'bae-m-lrow-lifted', props.dragging && 'bae-m-lrow-dragging') },
      props.lead ? h('span', { className: 'bae-muted bae-num bae-m-lrow-lead', style: lw != null ? { width: lw } : null }, props.lead) : null,
      h(Artwork, { title: props.album || props.title, size: props.cover ? Number(props.cover) : 32, radius: props.cover ? 6 : 4 }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-ellipsis', style: { fontWeight: 500 } }, props.title), h('div', { className: 'bae-muted bae-ellipsis bae-m-lrow-sub', style: { fontSize: 11.5 } }, props.sub)),
      props.who ? h(MacPersonTag, { name: props.who }) : null,
      props.duration ? h('span', { className: 'bae-muted bae-num', style: { width: 40, textAlign: 'right', fontSize: 11.5 } }, props.duration) : null);
  }
  // `menu`: the row ends in ⋮, the row's actions, labelled with `menu`'s text (More unless given).
  function AndroidListRow(props) {
    return h('div', { className: cx('bae-a-lrow', props.menu && 'bae-a-lrow-menu', props.dragging && 'bae-a-lrow-dragging') },
      h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-a-librow-title' }, props.title), h('div', { className: 'bae-a-librow-sub' }, props.sub)),
      props.who ? h(Avatar, { name: props.who, size: 24 }) : null,
      props.trailing ? h('span', { className: 'bae-a-fine bae-muted' }, props.trailing) : null,
      props.menu ? h('span', { className: 'bae-a-lrow-more' }, h(AndroidIconButton, { icon: 'more', label: typeof props.menu === 'string' ? props.menu : 'More', tone: 'muted' })) : null);
  }
