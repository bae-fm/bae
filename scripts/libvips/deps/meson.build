# The libraries libvips links, built as their own meson projects from the
# pinned wraps in subprojects/ and installed beside each other. libvips builds
# against the install through pkg-config (scripts/build-libvips.sh), so it
# sees them as external libraries, which its feature checks require.
project('bae-libvips-deps', 'c', version: '0')

subproject('zlib')
subproject('expat')
subproject('glib', default_options: [
  'tests=false', 'introspection=disabled', 'nls=disabled', 'libmount=disabled',
  'selinux=disabled', 'xattr=false', 'man-pages=disabled', 'dtrace=disabled',
  'sysprof=disabled', 'documentation=false', 'glib_debug=disabled',
])
# libvips registers dzsave, its tile pyramid writer, only when libarchive is
# there. Only zlib's gzip stays on in it.
libarchive = subproject('libarchive', default_options: [
  'progs=bsdtar', 'tests=disabled', 'bz2lib=disabled', 'libb2=disabled',
  'iconv=disabled', 'lz4=disabled', 'zstd=disabled', 'lzma=disabled',
  'lzo2=disabled', 'openssl=disabled', 'xml2=disabled', 'expat=disabled',
  'xattr=disabled', 'acl=disabled',
])
subproject('libjpeg-turbo')
subproject('libpng')
subproject('libtiff')
subproject('libwebp')
subproject('lcms2')
libexif = subproject('libexif')

# libarchive's and libexif's wraps install no .pc file, which libvips finds
# them through.
configure_file(
  input: 'libexif.pc.in',
  output: 'libexif.pc',
  configuration: {'version': libexif.get_variable('exif_dep').version()},
  install_dir: get_option('prefix') / get_option('libdir') / 'pkgconfig',
)
configure_file(
  input: 'libarchive.pc.in',
  output: 'libarchive.pc',
  configuration: {'version': libarchive.get_variable('libarchive_dep').version()},
  install_dir: get_option('prefix') / get_option('libdir') / 'pkgconfig',
)
