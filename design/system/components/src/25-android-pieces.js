  // ---- Android pieces: the album, artist and playlist pages, the gallery, the side pause, and the lines they share ----

  // An album's stars under its header, as AlbumRating draws them: the person's own rating (`value`, 0 for none), tappable, then who else rated it (`raters`) in fine text.
  function AndroidAlbumRating(props) {
    return h('div', { className: 'bae-a-rating' }, h(Stars, { value: props.value || 0, size: 24, rate: true }), props.raters ? h('span', { className: 'bae-a-fine bae-muted' }, props.raters) : null);
  }

  // An album or playlist page's play buttons: Play and Shuffle, then Play next and Add to queue on a second line. `short`: Play and Shuffle alone, as a playlist page has them.
  function AndroidPageActions(props) {
    var play = h('div', { className: 'bae-a-pageactions-line' }, h(AndroidButton, null, 'Play'), h(AndroidButton, { kind: 'outlined' }, 'Shuffle'));
    return h('div', { className: 'bae-a-pageactions' }, play,
      props.short ? null : h('div', { className: 'bae-a-pageactions-line' }, h(AndroidButton, { kind: 'outlined' }, 'Play next'), h(AndroidButton, { kind: 'outlined' }, 'Add to queue')));
  }

  // An album's credits at the foot of its page: the Credits eyebrow, then one line per credit, its role in secondary text at the start and its names at the end. `credits`: "Role:Names", comma separated.
  function AndroidAlbumCredits(props) {
    return h('div', { className: 'bae-a-credits' }, h('div', { className: 'bae-a-credits-head' }, h(AndroidEyebrow, null, 'Credits')),
      list(props.credits).map(function (c, i) { var at = c.indexOf(':'); return h('div', { key: i, className: 'bae-a-credit' }, h('span', { className: 'bae-muted' }, c.slice(0, at)), h('span', { className: 'bae-a-credit-names' }, c.slice(at + 1))); }));
  }

  // The release's audio as bae read it from its files (`children`, such as FLAC · 44.1 kHz · 16-bit · Stereo), one line in tertiary text at the foot of the album page, after its credits (decision #1232).
  function AndroidAlbumAudio(props) {
    return h('div', { className: 'bae-a-albumaudio' }, props.children);
  }

  // The other names an artist's tracks credit them by, under the artist's row: the Also credited as eyebrow over the names (`names`, comma separated), joined by middle dots in secondary text.
  function AndroidArtistOtherNames(props) {
    return h('div', { className: 'bae-a-othernames' }, h('div', { className: 'bae-a-othernames-head' }, h(AndroidEyebrow, null, 'Also credited as')),
      h('div', { className: 'bae-a-othernames-list' }, list(props.names).join(' · ')));
  }

  // A playlist page's head: its picture (`covers`), its name over the owner, track count and sharing (`facts`), then Play and Shuffle.
  function AndroidPlaylistHeader(props) {
    return h('div', { className: 'bae-a-plhead' },
      h('div', { className: 'bae-a-plhead-top' }, h(PlaylistPicture, { covers: props.covers, size: 96, radius: 10 }),
        h('div', null, h('div', { className: 'bae-a-plhead-name' }, props.name), h('div', { className: 'bae-a-fine bae-muted' }, props.facts))),
      h('div', { className: 'bae-a-plhead-actions' }, h(AndroidPageActions, { short: true })));
  }

  // The Pinning screen's choice of how many releases pin at once: Pins at once over one button per number up to `limit` (6 unless given), `selected` checked.
  function AndroidPinsAtOnce(props) {
    var n = Number(props.limit || 6), options = [];
    for (var i = 1; i <= n; i++) options.push(i);
    return h('div', { className: 'bae-a-pinsum' }, 'Pins at once', h(AndroidSegmentedControl, { options: options.join(','), selected: props.selected || '3' }));
  }

  // The choice of which of an album's releases its page shows, under the header: one chip per release (`releases`, comma separated), the one shown (`selected`) on the accent's tint, the others outlined.
  function AndroidReleasePicker(props) {
    return h('div', { className: 'bae-a-relpick', role: 'radiogroup', 'aria-label': 'Release' }, list(props.releases).map(function (r) {
      var on = r === props.selected;
      return h('span', { key: r, role: 'radio', 'aria-checked': on, className: cx('bae-a-relchip', on && 'bae-a-relchip-on') }, r);
    }));
  }

  // The release's Files on the album page: the files no track plays from (`files`: "name|kind" entries separated by semicolons), under the Files eyebrow, each a hairline over its name at the start and its kind at the end.
  function AndroidReleaseFiles(props) {
    var files = String(props.files || '').split(';').map(function (f) { return f.split('|').map(function (x) { return x.trim(); }); }).filter(function (f) { return f[0]; });
    return h('div', { className: 'bae-a-relfiles' },
      h('div', { className: 'bae-a-relfiles-head' }, h(AndroidEyebrow, null, 'Files')),
      files.map(function (f, i) {
        return h(React.Fragment, { key: i }, h(AndroidDivider), h('div', { className: 'bae-a-relfiles-row' }, h('span', null, f[0]), h('span', { className: 'bae-muted' }, f[1])));
      }));
  }

  // The camera screen that scans a restore code or an invite: the camera's picture fills the screen (drawn black), and at its foot the `instructions` on a scrim, when given, over Cancel.
  function AndroidQRScanner(props) {
    return h('div', { className: 'bae-a-screen bae-a-qrscan', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      h('div', { className: 'bae-a-qrscan-foot' },
        props.instructions ? h('div', { className: 'bae-a-qrscan-words' }, props.instructions) : null,
        h('div', { className: 'bae-a-qrscan-cancel' }, h(AndroidButton, null, 'Cancel'))));
  }

  // The gallery's caption, centred on the backdrop: the shown image's name (`label`), then its place among the release's images (`page` of `count`) when there are several. Android pages through the images by swiping, so the caption is all it shows.
  function AndroidGalleryCaption(props) {
    var count = Number(props.count || 1);
    return h('div', { className: 'bae-a-gallery-caption' }, h('div', null, props.label), count > 1 ? h('div', { className: 'bae-a-gallery-caption-count' }, (props.page || 1) + ' / ' + count) : null);
  }

  // A release's images, full screen on the backdrop, as GalleryDialog draws them: close at the top end, the image (`title` is the release whose art it stands in for) fitted to the width, or while it loads (`loading`) a spinner over Loading the image…, and the caption at the foot.
  function AndroidLightbox(props) {
    return h('div', { className: 'bae-a-screen bae-a-lightbox', role: 'dialog', 'aria-label': props.label },
      h('span', { className: 'bae-a-lightbox-close' }, h(AndroidIconButton, { icon: 'close', label: 'Close', tone: 'onfill' })),
      props.loading ? h('div', { className: 'bae-a-lightbox-wait' }, h(AndroidSpinner), h('span', { className: 'bae-a-lightbox-secondary' }, 'Loading the image…'))
        : h(Artwork, { title: props.title, label: props.title, size: 412, radius: 0 }),
      h(AndroidGalleryCaption, { label: props.label, page: props.page, count: props.count }));
  }

  // The dialog when playback pauses at the end of a side or disc, as MacSidePause: `kind` side or disc, `label` its name; `countdown`, the seconds until the next one starts; `unchecked` draws the box cleared, with where to turn the pause back on.
  function AndroidSidePause(props) {
    var disc = props.kind === 'disc';
    return h(AndroidDialog, { title: (disc ? 'Disc ' : 'Side ') + (props.label || (disc ? '1' : 'A')) + ' ended', width: 340 },
      h('div', { className: 'bae-a-sidepause' }, 'Playback is paused. Press Play to continue.',
        props.countdown ? h('div', { className: 'bae-a-sidepause-count' }, 'The next ' + (disc ? 'disc' : 'side') + ' starts in ' + props.countdown + ' seconds.') : null,
        h(AndroidCheckbox, { label: disc ? 'Pause between discs' : 'Pause between sides', on: !props.unchecked }),
        props.unchecked ? h('div', null, 'You can turn this back on in Settings › Playback.') : null),
      h(AndroidDialogActions, null, h(AndroidButton, { kind: 'text', neutral: true }, 'Close'), h(AndroidButton, null, 'Play')));
  }

  // A line saying what failed, in body text in the danger colour.
  function AndroidErrorLine(props) {
    return h('div', { className: 'bae-a-errline' }, props.children);
  }

  // The row piece that holds a glyph (`lead`) level with the first line of its text, as MacItem: the lead centred in a box one body line tall, which it may overhang, then the content, then `trail`.
  function AndroidItem(props) {
    var trail = kids(props.trail);
    return h('div', { className: 'bae-a-item' }, h('span', { className: 'bae-a-item-lead' }, props.lead), h('div', { className: 'bae-a-item-body' }, props.children), trail.length ? h('span', { className: 'bae-a-item-trail' }, trail) : null);
  }

  // Every catalog that describes a release, as a wrapped row of chips that each open the catalog's page, in the order core lists them (`records`: catalog names, comma separated).
  function AndroidRecordLinks(props) {
    return h('div', { className: 'bae-a-reclinks' }, list(props.records).map(function (r) { return h(StatusChip, { key: r }, r + ' ↗'); }));
  }

  // The album page's line where the pin control would be when this phone can't play the release, in core's words: where the release is (On Ana’s Mac).
  function AndroidUnplayableLine(props) {
    return h('span', { className: 'bae-a-fineline' }, props.children);
  }
