package fm.bae.app.data

/** A phone joins a library and never makes one, so a phone's library always has a cloud home; one without is a bug. */
class LibraryWithoutStorage : IllegalStateException("a phone's library has no cloud home")
