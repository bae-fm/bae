import { DEFAULT_LOCALE, LOCALES, NON_DEFAULT_LOCALES, localeInfo, localizePath } from './locales.mjs';

export { DEFAULT_LOCALE, LOCALES, NON_DEFAULT_LOCALES, localeInfo, localizePath };

export const sidebarTranslations = {
  sections: {
    gettingStarted: {
      en: 'Getting Started',
      es: 'Primeros pasos',
      fr: 'Premiers pas',
      de: 'Erste Schritte',
      'pt-BR': 'Primeiros passos',
      ja: 'はじめに',
      ko: '시작하기',
      'zh-Hans': '入门',
      ar: 'البدء',
      he: 'תחילת עבודה',
      uk: 'Початок роботи',
      bg: 'Първи стъпки',
      pl: 'Pierwsze kroki',
      cs: 'Začínáme',
      hr: 'Početak',
    },
    library: {
      en: 'Library',
      es: 'Biblioteca',
      fr: 'Bibliothèque',
      de: 'Bibliothek',
      'pt-BR': 'Biblioteca',
      ja: 'ライブラリ',
      ko: '라이브러리',
      'zh-Hans': '资料库',
      ar: 'المكتبة',
      he: 'ספרייה',
      uk: 'Бібліотека',
      bg: 'Библиотека',
      pl: 'Biblioteka',
      cs: 'Knihovna',
      hr: 'Biblioteka',
    },
    storage: {
      en: 'Storage',
      es: 'Almacenamiento',
      fr: 'Stockage',
      de: 'Speicher',
      'pt-BR': 'Armazenamento',
      ja: 'ストレージ',
      ko: '저장소',
      'zh-Hans': '存储',
      ar: 'التخزين',
      he: 'אחסון',
      uk: 'Сховище',
      bg: 'Съхранение',
      pl: 'Przechowywanie',
      cs: 'Úložiště',
      hr: 'Pohrana',
    },
    architecture: {
      en: 'Architecture',
      es: 'Arquitectura',
      fr: 'Architecture',
      de: 'Architektur',
      'pt-BR': 'Arquitetura',
      ja: 'アーキテクチャ',
      ko: '아키텍처',
      'zh-Hans': '架构',
      ar: 'البنية',
      he: 'ארכיטקטורה',
      uk: 'Архітектура',
      bg: 'Архитектура',
      pl: 'Architektura',
      cs: 'Architektura',
      hr: 'Arhitektura',
    },
  },
  pages: {
    installation: {
      en: 'Installation',
      es: 'Instalación',
      fr: 'Installation',
      de: 'Installation',
      'pt-BR': 'Instalação',
      ja: 'インストール',
      ko: '설치',
      'zh-Hans': '安装',
      ar: 'التثبيت',
      he: 'התקנה',
      uk: 'Встановлення',
      bg: 'Инсталиране',
      pl: 'Instalacja',
      cs: 'Instalace',
      hr: 'Instalacija',
    },
    quickStart: {
      en: 'Quick Start',
      es: 'Inicio rápido',
      fr: 'Démarrage rapide',
      de: 'Schnellstart',
      'pt-BR': 'Início rápido',
      ja: 'クイックスタート',
      ko: '빠른 시작',
      'zh-Hans': '快速开始',
      ar: 'بدء سريع',
      he: 'התחלה מהירה',
      uk: 'Швидкий старт',
      bg: 'Бърз старт',
      pl: 'Szybki start',
      cs: 'Rychlý start',
      hr: 'Brzi početak',
    },
    importing: {
      en: 'Importing',
      es: 'Importación',
      fr: 'Importation',
      de: 'Importieren',
      'pt-BR': 'Importação',
      ja: 'インポート',
      ko: '가져오기',
      'zh-Hans': '导入',
      ar: 'الاستيراد',
      he: 'ייבוא',
      uk: 'Імпорт',
      bg: 'Импортиране',
      pl: 'Importowanie',
      cs: 'Import',
      hr: 'Uvoz',
    },
    metadata: {
      en: 'Metadata',
      es: 'Metadatos',
      fr: 'Métadonnées',
      de: 'Metadaten',
      'pt-BR': 'Metadados',
      ja: 'メタデータ',
      ko: '메타데이터',
      'zh-Hans': '元数据',
      ar: 'البيانات الوصفية',
      he: 'מטא-נתונים',
      uk: 'Метадані',
      bg: 'Метаданни',
      pl: 'Metadane',
      cs: 'Metadata',
      hr: 'Metapodaci',
    },
    browsing: {
      en: 'Browsing',
      es: 'Exploración',
      fr: 'Navigation',
      de: 'Durchsuchen',
      'pt-BR': 'Navegação',
      ja: 'ブラウズ',
      ko: '둘러보기',
      'zh-Hans': '浏览',
      ar: 'التصفح',
      he: 'עיון',
      uk: 'Перегляд',
      bg: 'Преглеждане',
      pl: 'Przeglądanie',
      cs: 'Procházení',
      hr: 'Pregledavanje',
    },
    overview: {
      en: 'Overview',
      es: 'Resumen',
      fr: 'Aperçu',
      de: 'Überblick',
      'pt-BR': 'Visão geral',
      ja: '概要',
      ko: '개요',
      'zh-Hans': '概览',
      ar: 'نظرة عامة',
      he: 'סקירה',
      uk: 'Огляд',
      bg: 'Общ преглед',
      pl: 'Omówienie',
      cs: 'Přehled',
      hr: 'Pregled',
    },
    sync: {
      en: 'Sync',
      es: 'Sincronización',
      fr: 'Synchronisation',
      de: 'Synchronisierung',
      'pt-BR': 'Sincronização',
      ja: '同期',
      ko: '동기화',
      'zh-Hans': '同步',
      ar: 'المزامنة',
      he: 'סנכרון',
      uk: 'Синхронізація',
      bg: 'Синхронизиране',
      pl: 'Synchronizacja',
      cs: 'Synchronizace',
      hr: 'Sinkronizacija',
    },
    dataModel: {
      en: 'Data Model',
      es: 'Modelo de datos',
      fr: 'Modèle de données',
      de: 'Datenmodell',
      'pt-BR': 'Modelo de dados',
      ja: 'データモデル',
      ko: '데이터 모델',
      'zh-Hans': '数据模型',
      ar: 'نموذج البيانات',
      he: 'מודל נתונים',
      uk: 'Модель даних',
      bg: 'Модел на данните',
      pl: 'Model danych',
      cs: 'Datový model',
      hr: 'Model podataka',
    },
    cloudHome: {
      en: 'Cloud Home',
      es: 'Inicio en la nube',
      fr: 'Emplacement cloud',
      de: 'Cloud-Speicherort',
      'pt-BR': 'Local na nuvem',
      ja: 'クラウドホーム',
      ko: '클라우드 홈',
      'zh-Hans': '云端位置',
      ar: 'موقع السحابة',
      he: 'בית ענן',
      uk: 'Хмарний дім',
      bg: 'Облачен дом',
      pl: 'Dom w chmurze',
      cs: 'Cloud Home',
      hr: 'Cloud Home',
    },
    encryption: {
      en: 'Encryption',
      es: 'Cifrado',
      fr: 'Chiffrement',
      de: 'Verschlüsselung',
      'pt-BR': 'Criptografia',
      ja: '暗号化',
      ko: '암호화',
      'zh-Hans': '加密',
      ar: 'التشفير',
      he: 'הצפנה',
      uk: 'Шифрування',
      bg: 'Шифроване',
      pl: 'Szyfrowanie',
      cs: 'Šifrování',
      hr: 'Šifriranje',
    },
    membership: {
      en: 'Membership',
      es: 'Membresía',
      fr: 'Appartenance',
      de: 'Mitgliedschaft',
      'pt-BR': 'Associação',
      ja: 'メンバーシップ',
      ko: '멤버십',
      'zh-Hans': '成员资格',
      ar: 'العضوية',
      he: 'חברות',
      uk: 'Учасники',
      bg: 'Членство',
      pl: 'Członkostwo',
      cs: 'Členství',
      hr: 'Članstvo',
    },
    serverless: {
      en: 'Serverless',
      es: 'Sin servidor',
      fr: 'Sans serveur',
      de: 'Serverlos',
      'pt-BR': 'Sem servidor',
      ja: 'サーバーレス',
      ko: '서버리스',
      'zh-Hans': '无服务器',
      ar: 'بلا خادم',
      he: 'ללא שרת',
      uk: 'Без сервера',
      bg: 'Без сървър',
      pl: 'Bez serwera',
      cs: 'Bez serveru',
      hr: 'Bez poslužitelja',
    },
  },
};

export const landing = {
  en: {
    metaDescription:
      'bae is a music library for the albums you own, on your phone and desktop, synced through cloud storage you already have. bae is pre-1.0 and not ready for general use.',
    title: 'bae: your music everywhere',
    navDocs: 'Docs',
    navDownload: 'Download',
    languageLabel: 'Language',
    heroTitle: ['Your music', 'everywhere'],
    heroText:
      'All your music, on your phone and desktop. Listen across devices, synced through your cloud storage. No server to keep running.',
    heroBold: 'Private, yours.',
    statusLabel: 'bae development status',
    statusKicker: 'Pre-1.0',
    statusTitle: 'Not ready for general use.',
    statusText: 'Testing builds only. Data and sync formats will change without migration.',
    downloadMac: 'Download for macOS',
    seeFeatures: 'See features',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Yours', 'your files stay yours'],
      ['Private', 'your devices hold the keys'],
      ['Everywhere', 'synced through your cloud'],
    ],
    sheepAlt: 'bae the sheep, wearing headphones',
    noteSynced: 'Synced',
    noteSyncedSub: 'across your devices',
    noteCloud: 'Your cloud',
    noteCloudSub: 'no server',
    syncEyebrow: 'Encrypted sync',
    syncTitle: ['Yours alone, on', 'every device'],
    syncText:
      'Your devices sync through cloud storage you choose. No server to run. Everything is encrypted before it leaves your device. Only you can read it.',
    desktop: 'Your desktop',
    phone: 'Your phone',
    holdsKeys: 'holds the keys',
    encrypted: 'encrypted',
    cloudStorage: 'Your cloud storage',
    encryptionLink: 'See how the encryption works →',
    libraryEyebrow: 'Library',
    libraryTitle: ['Your whole collection,', 'release-accurate'],
    libraryText: 'Bring the albums you own into one library, then take them anywhere.',
    cards: [
      ['Releases, not folders', 'Point it at your music folders, and bae helps match each to the right release. Different releases of the same album are grouped together.', ['Metadata sources', 'Cover art', 'Releases grouped']],
      ['Whole library, every device', 'Your cloud holds the library. Every device sees it all. Tracks download as they play; pins play offline.', ['Phone & desktop', 'Offline pins']],
      ['Playback details', 'Albums keep their playback details: vinyl and cassette recordings pause on side breaks. CUE sheets enable playing pregaps from CDs.', ['CD pregaps', 'Vinyl side breaks']],
      ['You control it', 'Your music stays yours. Your files stay where you put them. You control your library.', ['Your files', 'No lock-in']],
    ],
    engineEyebrow: 'Under the hood',
    engineTitle: ['Native apps on', 'every platform'],
    engineText:
      'Each app is native to its platform. They all use the same Rust core for your library, playback, sync, and encryption.',
    rustCore: 'Rust core',
    rustCoreSub: 'library · playback · sync · encryption',
    sharedRustCore: 'shared Rust core',
    deps: [
      ['FFmpeg', 'audio decoding'],
      ['SQLite', 'library database'],
      ['Cloud storage', 'sync backend'],
    ],
    playbackEyebrow: 'Playback',
    playbackTitle: 'Built around your files',
    minis: [
      ['Original quality', 'Your files stay exactly as they are. Playback uses the original files.'],
      ['Local first', 'Match folders to releases without changing your files. Add cloud storage when you want sync.'],
      ['Plays offline', 'Pin releases to your device. They keep playing offline.'],
    ],
    endAria: 'Bring your collection home',
    endPrefix: 'Bring your collection',
    endFallback: 'home.',
    endText: 'Private, open source, and in active development. Everything you add stays yours.',
    readArchitecture: 'Read architecture',
    footerStatus: 'private & open source',
    cycleWords: ['home', 'everywhere', 'together', 'offline', 'under control'],
  },
  es: {
    metaDescription:
      'bae es una biblioteca musical para los álbumes que posees, en tu teléfono y escritorio, sincronizada mediante el almacenamiento en la nube que ya tienes. bae es anterior a la versión 1.0 y no está lista para uso general.',
    title: 'bae: tu música en todas partes',
    navDocs: 'Docs',
    navDownload: 'Descargar',
    languageLabel: 'Idioma',
    heroTitle: ['Tu música', 'en todas partes'],
    heroText:
      'Toda tu música, en el teléfono y en el escritorio. Escucha en varios dispositivos, sincronizados mediante tu almacenamiento en la nube. Sin servidor que mantener.',
    heroBold: 'Privada, tuya.',
    statusLabel: 'estado de desarrollo de bae',
    statusKicker: 'Pre-1.0',
    statusTitle: 'No está lista para uso general.',
    statusText: 'Solo compilaciones de prueba. Los formatos de datos y sincronización cambiarán sin migración.',
    downloadMac: 'Descargar para macOS',
    seeFeatures: 'Ver funciones',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Tuya', 'tus archivos siguen siendo tuyos'],
      ['Privada', 'tus dispositivos guardan las claves'],
      ['En todas partes', 'sincronizada mediante tu nube'],
    ],
    sheepAlt: 'la oveja de bae con auriculares',
    noteSynced: 'Sincronizada',
    noteSyncedSub: 'entre tus dispositivos',
    noteCloud: 'Tu nube',
    noteCloudSub: 'sin servidor',
    syncEyebrow: 'Sincronización cifrada',
    syncTitle: ['Solo tuya, en', 'cada dispositivo'],
    syncText:
      'Tus dispositivos se sincronizan mediante el almacenamiento en la nube que eliges. Sin servidor que ejecutar. Todo se cifra antes de salir del dispositivo. Solo tú puedes leerlo.',
    desktop: 'Tu escritorio',
    phone: 'Tu teléfono',
    holdsKeys: 'guarda las claves',
    encrypted: 'cifrado',
    cloudStorage: 'Tu almacenamiento en la nube',
    encryptionLink: 'Ver cómo funciona el cifrado →',
    libraryEyebrow: 'Biblioteca',
    libraryTitle: ['Toda tu colección,', 'fiel a cada edición'],
    libraryText: 'Reúne los álbumes que posees en una biblioteca y llévalos contigo.',
    cards: [
      ['Ediciones, no carpetas', 'Apunta bae a tus carpetas de música y te ayuda a emparejar cada una con la edición correcta. Distintas ediciones del mismo álbum se agrupan juntas.', ['Fuentes de metadatos', 'Portadas', 'Ediciones agrupadas']],
      ['Toda la biblioteca, en cada dispositivo', 'Tu nube guarda la biblioteca. Cada dispositivo la ve completa. Las pistas se descargan al reproducirse; los anclajes funcionan sin conexión.', ['Teléfono y escritorio', 'Anclajes sin conexión']],
      ['Detalles de reproducción', 'Los álbumes conservan sus detalles de reproducción: las grabaciones de vinilo y casete pausan entre caras. Las hojas CUE permiten reproducir pregaps de CD.', ['Pregaps de CD', 'Cambios de cara de vinilo']],
      ['Tú tienes el control', 'Tu música sigue siendo tuya. Tus archivos se quedan donde los pones. Tú controlas tu biblioteca.', ['Tus archivos', 'Sin encierro']],
    ],
    engineEyebrow: 'Por dentro',
    engineTitle: ['Apps nativas en', 'cada plataforma'],
    engineText:
      'Cada app es nativa de su plataforma. Todas usan el mismo núcleo Rust para tu biblioteca, reproducción, sincronización y cifrado.',
    rustCore: 'Núcleo Rust',
    rustCoreSub: 'biblioteca · reproducción · sincronización · cifrado',
    sharedRustCore: 'núcleo Rust compartido',
    deps: [
      ['FFmpeg', 'decodificación de audio'],
      ['SQLite', 'base de datos de la biblioteca'],
      ['Almacenamiento en la nube', 'backend de sincronización'],
    ],
    playbackEyebrow: 'Reproducción',
    playbackTitle: 'Diseñada alrededor de tus archivos',
    minis: [
      ['Calidad original', 'Tus archivos permanecen exactamente como están. La reproducción usa los archivos originales.'],
      ['Local primero', 'Empareja carpetas con ediciones sin cambiar tus archivos. Añade almacenamiento en la nube cuando quieras sincronizar.'],
      ['Reproduce sin conexión', 'Ancla ediciones a tu dispositivo. Siguen reproduciéndose sin conexión.'],
    ],
    endAria: 'Trae tu colección a casa',
    endPrefix: 'Trae tu colección',
    endFallback: 'a casa.',
    endText: 'Privada, de código abierto y en desarrollo activo. Todo lo que añades sigue siendo tuyo.',
    readArchitecture: 'Leer arquitectura',
    footerStatus: 'privada y de código abierto',
    cycleWords: ['a casa', 'a todas partes', 'junta', 'sin conexión', 'bajo control'],
  },
  fr: {
    metaDescription:
      "bae est une bibliothèque musicale pour les albums que vous possédez, sur téléphone et ordinateur, synchronisée via le stockage cloud que vous avez déjà. bae est pré-1.0 et n'est pas prête pour un usage général.",
    title: 'bae : votre musique partout',
    navDocs: 'Docs',
    navDownload: 'Télécharger',
    languageLabel: 'Langue',
    heroTitle: ['Votre musique', 'partout'],
    heroText:
      "Toute votre musique, sur téléphone et ordinateur. Écoutez sur plusieurs appareils, synchronisés via votre stockage cloud. Aucun serveur à maintenir.",
    heroBold: 'Privée, à vous.',
    statusLabel: 'état de développement de bae',
    statusKicker: 'Pré-1.0',
    statusTitle: "Pas prête pour un usage général.",
    statusText: 'Versions de test uniquement. Les formats de données et de synchronisation changeront sans migration.',
    downloadMac: 'Télécharger pour macOS',
    seeFeatures: 'Voir les fonctions',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['À vous', 'vos fichiers restent à vous'],
      ['Privée', 'vos appareils gardent les clés'],
      ['Partout', 'synchronisée via votre cloud'],
    ],
    sheepAlt: 'le mouton bae avec un casque',
    noteSynced: 'Synchronisée',
    noteSyncedSub: 'sur vos appareils',
    noteCloud: 'Votre cloud',
    noteCloudSub: 'sans serveur',
    syncEyebrow: 'Synchronisation chiffrée',
    syncTitle: ['À vous seule, sur', 'chaque appareil'],
    syncText:
      "Vos appareils se synchronisent via le stockage cloud que vous choisissez. Aucun serveur à exécuter. Tout est chiffré avant de quitter votre appareil. Vous seul pouvez le lire.",
    desktop: 'Votre ordinateur',
    phone: 'Votre téléphone',
    holdsKeys: 'garde les clés',
    encrypted: 'chiffré',
    cloudStorage: 'Votre stockage cloud',
    encryptionLink: 'Voir comment fonctionne le chiffrement →',
    libraryEyebrow: 'Bibliothèque',
    libraryTitle: ['Toute votre collection,', 'fidèle aux éditions'],
    libraryText: 'Rassemblez les albums que vous possédez dans une bibliothèque, puis emportez-les partout.',
    cards: [
      ['Des éditions, pas des dossiers', "Indiquez vos dossiers de musique à bae, qui aide à associer chacun à la bonne édition. Les différentes éditions d'un même album sont regroupées.", ['Sources de métadonnées', 'Pochettes', 'Éditions regroupées']],
      ['Toute la bibliothèque, sur chaque appareil', "Votre cloud contient la bibliothèque. Chaque appareil la voit en entier. Les pistes se téléchargent à la lecture ; les épingles fonctionnent hors ligne.", ['Téléphone et ordinateur', 'Épingles hors ligne']],
      ['Détails de lecture', 'Les albums conservent leurs détails de lecture : les enregistrements vinyle et cassette marquent une pause aux changements de face. Les feuilles CUE permettent de lire les pregaps de CD.', ['Pregaps de CD', 'Changements de face vinyle']],
      ['Vous la contrôlez', 'Votre musique reste à vous. Vos fichiers restent là où vous les mettez. Vous contrôlez votre bibliothèque.', ['Vos fichiers', 'Pas de verrouillage']],
    ],
    engineEyebrow: 'Sous le capot',
    engineTitle: ['Apps natives sur', 'chaque plateforme'],
    engineText:
      'Chaque app est native de sa plateforme. Toutes utilisent le même noyau Rust pour votre bibliothèque, la lecture, la synchronisation et le chiffrement.',
    rustCore: 'Noyau Rust',
    rustCoreSub: 'bibliothèque · lecture · synchronisation · chiffrement',
    sharedRustCore: 'noyau Rust partagé',
    deps: [
      ['FFmpeg', 'décodage audio'],
      ['SQLite', 'base de données de bibliothèque'],
      ['Stockage cloud', 'backend de synchronisation'],
    ],
    playbackEyebrow: 'Lecture',
    playbackTitle: 'Conçue autour de vos fichiers',
    minis: [
      ['Qualité originale', 'Vos fichiers restent exactement tels quels. La lecture utilise les fichiers originaux.'],
      ['Local d’abord', 'Associez des dossiers à des éditions sans modifier vos fichiers. Ajoutez le stockage cloud quand vous voulez synchroniser.'],
      ['Lecture hors ligne', 'Épinglez des éditions sur votre appareil. Elles continuent à jouer hors ligne.'],
    ],
    endAria: 'Ramenez votre collection chez vous',
    endPrefix: 'Ramenez votre collection',
    endFallback: 'chez vous.',
    endText: 'Privée, open source et en développement actif. Tout ce que vous ajoutez reste à vous.',
    readArchitecture: 'Lire l’architecture',
    footerStatus: 'privée et open source',
    cycleWords: ['chez vous', 'partout', 'ensemble', 'hors ligne', 'sous contrôle'],
  },
  de: {
    metaDescription:
      'bae ist eine Musikbibliothek für die Alben, die dir gehören, auf Telefon und Desktop, synchronisiert über den Cloud-Speicher, den du bereits hast. bae ist vor Version 1.0 und nicht für die allgemeine Nutzung bereit.',
    title: 'bae: deine Musik überall',
    navDocs: 'Docs',
    navDownload: 'Herunterladen',
    languageLabel: 'Sprache',
    heroTitle: ['Deine Musik', 'überall'],
    heroText:
      'Deine ganze Musik auf Telefon und Desktop. Höre auf mehreren Geräten, synchronisiert über deinen Cloud-Speicher. Kein Server, den du betreiben musst.',
    heroBold: 'Privat, deine.',
    statusLabel: 'Entwicklungsstand von bae',
    statusKicker: 'Vor 1.0',
    statusTitle: 'Nicht für die allgemeine Nutzung bereit.',
    statusText: 'Nur Test-Builds. Daten- und Sync-Formate ändern sich ohne Migration.',
    downloadMac: 'Für macOS herunterladen',
    seeFeatures: 'Funktionen ansehen',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Deine', 'deine Dateien bleiben deine'],
      ['Privat', 'deine Geräte halten die Schlüssel'],
      ['Überall', 'über deine Cloud synchronisiert'],
    ],
    sheepAlt: 'das bae-Schaf mit Kopfhörern',
    noteSynced: 'Synchronisiert',
    noteSyncedSub: 'auf deinen Geräten',
    noteCloud: 'Deine Cloud',
    noteCloudSub: 'kein Server',
    syncEyebrow: 'Verschlüsselter Sync',
    syncTitle: ['Nur deine, auf', 'jedem Gerät'],
    syncText:
      'Deine Geräte synchronisieren über den Cloud-Speicher, den du wählst. Kein Serverbetrieb. Alles wird verschlüsselt, bevor es dein Gerät verlässt. Nur du kannst es lesen.',
    desktop: 'Dein Desktop',
    phone: 'Dein Telefon',
    holdsKeys: 'hält die Schlüssel',
    encrypted: 'verschlüsselt',
    cloudStorage: 'Dein Cloud-Speicher',
    encryptionLink: 'So funktioniert die Verschlüsselung →',
    libraryEyebrow: 'Bibliothek',
    libraryTitle: ['Deine ganze Sammlung,', 'releasegenau'],
    libraryText: 'Bringe die Alben, die dir gehören, in eine Bibliothek und nimm sie überall mit.',
    cards: [
      ['Veröffentlichungen statt Ordner', 'Zeige bae deine Musikordner, und bae hilft, jeden der richtigen Veröffentlichung zuzuordnen. Verschiedene Veröffentlichungen desselben Albums werden gruppiert.', ['Metadatenquellen', 'Coverbilder', 'Veröffentlichungen gruppiert']],
      ['Ganze Bibliothek, jedes Gerät', 'Deine Cloud hält die Bibliothek. Jedes Gerät sieht sie vollständig. Titel werden beim Abspielen geladen; angeheftete Veröffentlichungen spielen offline.', ['Telefon und Desktop', 'Offline angeheftet']],
      ['Wiedergabedetails', 'Alben behalten ihre Wiedergabedetails: Vinyl- und Kassettenaufnahmen pausieren bei Seitenwechseln. CUE-Dateien ermöglichen CD-Pregaps.', ['CD-Pregaps', 'Vinyl-Seitenwechsel']],
      ['Du kontrollierst sie', 'Deine Musik bleibt deine. Deine Dateien bleiben dort, wo du sie ablegst. Du kontrollierst deine Bibliothek.', ['Deine Dateien', 'Keine Bindung']],
    ],
    engineEyebrow: 'Unter der Haube',
    engineTitle: ['Native Apps auf', 'jeder Plattform'],
    engineText:
      'Jede App ist nativ für ihre Plattform. Alle verwenden denselben Rust-Kern für Bibliothek, Wiedergabe, Sync und Verschlüsselung.',
    rustCore: 'Rust-Kern',
    rustCoreSub: 'Bibliothek · Wiedergabe · Sync · Verschlüsselung',
    sharedRustCore: 'gemeinsamer Rust-Kern',
    deps: [
      ['FFmpeg', 'Audio-Decoding'],
      ['SQLite', 'Bibliotheksdatenbank'],
      ['Cloud-Speicher', 'Sync-Backend'],
    ],
    playbackEyebrow: 'Wiedergabe',
    playbackTitle: 'Auf deine Dateien ausgelegt',
    minis: [
      ['Originalqualität', 'Deine Dateien bleiben genau so, wie sie sind. Die Wiedergabe nutzt die Originaldateien.'],
      ['Lokal zuerst', 'Ordne Ordner Veröffentlichungen zu, ohne deine Dateien zu ändern. Füge Cloud-Speicher hinzu, wenn du synchronisieren willst.'],
      ['Spielt offline', 'Hefte Veröffentlichungen an dein Gerät an. Sie spielen weiter offline.'],
    ],
    endAria: 'Bring deine Sammlung nach Hause',
    endPrefix: 'Bring deine Sammlung',
    endFallback: 'nach Hause.',
    endText: 'Privat, Open Source und in aktiver Entwicklung. Alles, was du hinzufügst, bleibt deins.',
    readArchitecture: 'Architektur lesen',
    footerStatus: 'privat und Open Source',
    cycleWords: ['nach Hause', 'überallhin', 'zusammen', 'offline', 'unter Kontrolle'],
  },
  'pt-BR': {
    metaDescription:
      'bae é uma biblioteca musical para os álbuns que você possui, no celular e no desktop, sincronizada pelo armazenamento em nuvem que você já tem. bae é pré-1.0 e não está pronta para uso geral.',
    title: 'bae: sua música em todos os lugares',
    navDocs: 'Docs',
    navDownload: 'Baixar',
    languageLabel: 'Idioma',
    heroTitle: ['Sua música', 'em todos os lugares'],
    heroText:
      'Toda a sua música, no celular e no desktop. Ouça em vários dispositivos, sincronizados pelo seu armazenamento em nuvem. Nenhum servidor para manter.',
    heroBold: 'Privada, sua.',
    statusLabel: 'estado de desenvolvimento do bae',
    statusKicker: 'Pré-1.0',
    statusTitle: 'Não está pronta para uso geral.',
    statusText: 'Somente builds de teste. Os formatos de dados e sincronização mudarão sem migração.',
    downloadMac: 'Baixar para macOS',
    seeFeatures: 'Ver recursos',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Sua', 'seus arquivos continuam seus'],
      ['Privada', 'seus dispositivos guardam as chaves'],
      ['Em todos os lugares', 'sincronizada pela sua nuvem'],
    ],
    sheepAlt: 'a ovelha do bae usando fones',
    noteSynced: 'Sincronizada',
    noteSyncedSub: 'entre seus dispositivos',
    noteCloud: 'Sua nuvem',
    noteCloudSub: 'sem servidor',
    syncEyebrow: 'Sincronização criptografada',
    syncTitle: ['Só sua, em', 'cada dispositivo'],
    syncText:
      'Seus dispositivos sincronizam pelo armazenamento em nuvem que você escolhe. Nenhum servidor para executar. Tudo é criptografado antes de sair do dispositivo. Só você pode ler.',
    desktop: 'Seu desktop',
    phone: 'Seu celular',
    holdsKeys: 'guarda as chaves',
    encrypted: 'criptografado',
    cloudStorage: 'Seu armazenamento em nuvem',
    encryptionLink: 'Ver como a criptografia funciona →',
    libraryEyebrow: 'Biblioteca',
    libraryTitle: ['Toda a sua coleção,', 'fiel à edição'],
    libraryText: 'Coloque os álbuns que você possui em uma biblioteca e leve-os com você.',
    cards: [
      ['Edições, não pastas', 'Aponte para suas pastas de música, e o bae ajuda a combinar cada uma com a edição correta. Edições diferentes do mesmo álbum ficam agrupadas.', ['Fontes de metadados', 'Capas', 'Edições agrupadas']],
      ['Biblioteca inteira, todos os dispositivos', 'Sua nuvem guarda a biblioteca. Cada dispositivo vê tudo. As faixas baixam ao tocar; itens fixados tocam offline.', ['Celular e desktop', 'Fixados offline']],
      ['Detalhes de reprodução', 'Os álbuns mantêm detalhes de reprodução: gravações de vinil e fita pausam nas viradas de lado. Folhas CUE permitem tocar pregaps de CDs.', ['Pregaps de CD', 'Viradas de lado do vinil']],
      ['Você controla', 'Sua música continua sua. Seus arquivos ficam onde você os colocou. Você controla sua biblioteca.', ['Seus arquivos', 'Sem aprisionamento']],
    ],
    engineEyebrow: 'Por dentro',
    engineTitle: ['Apps nativos em', 'cada plataforma'],
    engineText:
      'Cada app é nativo da sua plataforma. Todos usam o mesmo núcleo Rust para biblioteca, reprodução, sincronização e criptografia.',
    rustCore: 'Núcleo Rust',
    rustCoreSub: 'biblioteca · reprodução · sincronização · criptografia',
    sharedRustCore: 'núcleo Rust compartilhado',
    deps: [
      ['FFmpeg', 'decodificação de áudio'],
      ['SQLite', 'banco de dados da biblioteca'],
      ['Armazenamento em nuvem', 'backend de sincronização'],
    ],
    playbackEyebrow: 'Reprodução',
    playbackTitle: 'Feita ao redor dos seus arquivos',
    minis: [
      ['Qualidade original', 'Seus arquivos ficam exatamente como estão. A reprodução usa os arquivos originais.'],
      ['Local primeiro', 'Combine pastas com edições sem alterar seus arquivos. Adicione nuvem quando quiser sincronizar.'],
      ['Toca offline', 'Fixe edições no dispositivo. Elas continuam tocando offline.'],
    ],
    endAria: 'Traga sua coleção para casa',
    endPrefix: 'Traga sua coleção',
    endFallback: 'para casa.',
    endText: 'Privada, open source e em desenvolvimento ativo. Tudo que você adiciona continua seu.',
    readArchitecture: 'Ler arquitetura',
    footerStatus: 'privada e open source',
    cycleWords: ['para casa', 'a todos os lugares', 'junta', 'offline', 'sob controle'],
  },
  ja: {
    metaDescription:
      'bae は、所有しているアルバムを電話とデスクトップで扱う音楽ライブラリです。すでに持っているクラウドストレージで同期します。bae は 1.0 前で、一般利用向けではありません。',
    title: 'bae: あなたの音楽をどこでも',
    navDocs: 'ドキュメント',
    navDownload: 'ダウンロード',
    languageLabel: '言語',
    heroTitle: ['あなたの音楽を', 'どこでも'],
    heroText:
      'すべての音楽を電話とデスクトップで。自分のクラウドストレージを通じて同期し、複数のデバイスで聴けます。動かし続けるサーバーはありません。',
    heroBold: '非公開で、あなたのもの。',
    statusLabel: 'bae の開発状況',
    statusKicker: 'Pre-1.0',
    statusTitle: '一般利用向けではありません。',
    statusText: 'テストビルドのみです。データ形式と同期形式は移行なしで変更されます。',
    downloadMac: 'macOS 版をダウンロード',
    seeFeatures: '機能を見る',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['あなたのもの', 'ファイルはあなたのものです'],
      ['非公開', '鍵はあなたのデバイスが保持します'],
      ['どこでも', 'あなたのクラウドで同期します'],
    ],
    sheepAlt: 'ヘッドホンをつけた bae の羊',
    noteSynced: '同期済み',
    noteSyncedSub: 'あなたのデバイス間で',
    noteCloud: 'あなたのクラウド',
    noteCloudSub: 'サーバーなし',
    syncEyebrow: '暗号化同期',
    syncTitle: ['あなただけのものを', 'すべてのデバイスで'],
    syncText:
      'デバイスは、あなたが選ぶクラウドストレージを通じて同期します。実行するサーバーはありません。すべてはデバイスを出る前に暗号化され、読めるのはあなただけです。',
    desktop: 'あなたのデスクトップ',
    phone: 'あなたの電話',
    holdsKeys: '鍵を保持',
    encrypted: '暗号化',
    cloudStorage: 'あなたのクラウドストレージ',
    encryptionLink: '暗号化の仕組みを見る →',
    libraryEyebrow: 'ライブラリ',
    libraryTitle: ['あなたのコレクション全体を', 'リリース単位で正確に'],
    libraryText: '所有しているアルバムを 1 つのライブラリにまとめ、どこへでも持ち出せます。',
    cards: [
      ['フォルダではなくリリース', '音楽フォルダを指定すると、bae がそれぞれを正しいリリースに対応付けます。同じアルバムの別リリースは一緒にグループ化されます。', ['メタデータソース', 'カバーアート', 'リリースをグループ化']],
      ['ライブラリ全体をすべてのデバイスで', 'クラウドがライブラリを保持します。どのデバイスからも全体が見えます。トラックは再生時にダウンロードされ、ピン留めしたものはオフラインで再生できます。', ['電話とデスクトップ', 'オフラインピン']],
      ['再生の詳細', 'アルバムは再生上の詳細を保ちます。レコードやカセット録音は面の切り替わりで一時停止します。CUE シートにより CD のプリギャップを再生できます。', ['CD プリギャップ', 'レコードの面切り替え']],
      ['自分で管理', '音楽はあなたのものです。ファイルは置いた場所に残ります。ライブラリはあなたが管理します。', ['あなたのファイル', '閉じ込めなし']],
    ],
    engineEyebrow: '内部',
    engineTitle: ['すべてのプラットフォームで', 'ネイティブアプリ'],
    engineText:
      '各アプリはそのプラットフォーム向けのネイティブアプリです。ライブラリ、再生、同期、暗号化には同じ Rust コアを使います。',
    rustCore: 'Rust コア',
    rustCoreSub: 'ライブラリ · 再生 · 同期 · 暗号化',
    sharedRustCore: '共有 Rust コア',
    deps: [
      ['FFmpeg', '音声デコード'],
      ['SQLite', 'ライブラリデータベース'],
      ['クラウドストレージ', '同期バックエンド'],
    ],
    playbackEyebrow: '再生',
    playbackTitle: 'あなたのファイルを中心に',
    minis: [
      ['元の品質', 'ファイルはそのまま保持されます。再生には元のファイルを使います。'],
      ['ローカル優先', 'ファイルを変更せずにフォルダをリリースに対応付けます。同期したいときにクラウドストレージを追加できます。'],
      ['オフライン再生', 'リリースをデバイスにピン留めできます。オフラインでも再生できます。'],
    ],
    endAria: 'コレクションを手元へ',
    endPrefix: 'コレクションを',
    endFallback: '手元へ。',
    endText: '非公開、オープンソース、現在開発中です。追加したものはすべてあなたのものです。',
    readArchitecture: 'アーキテクチャを読む',
    footerStatus: '非公開・オープンソース',
    cycleWords: ['手元へ', 'どこでも', '一緒に', 'オフラインで', '自分の管理下に'],
  },

  ko: {
    metaDescription:
      'bae는 소유한 앨범을 휴대폰과 데스크톱에서 관리하는 음악 라이브러리입니다. 이미 쓰고 있는 클라우드 저장소로 동기화합니다. bae는 1.0 이전 버전이며 일반 사용에 준비되지 않았습니다.',
    title: 'bae: 내 음악을 어디서나',
    navDocs: '문서',
    navDownload: '다운로드',
    languageLabel: '언어',
    heroTitle: ['내 음악을', '어디서나'],
    heroText:
      '내 모든 음악을 휴대폰과 데스크톱에서 들으세요. 내 클라우드 저장소를 통해 기기 간에 동기화됩니다. 계속 실행할 서버가 없습니다.',
    heroBold: '비공개, 내 것.',
    statusLabel: 'bae 개발 상태',
    statusKicker: 'Pre-1.0',
    statusTitle: '일반 사용에 준비되지 않았습니다.',
    statusText: '테스트 빌드만 제공합니다. 데이터와 동기화 형식은 마이그레이션 없이 바뀔 수 있습니다.',
    downloadMac: 'macOS용 다운로드',
    seeFeatures: '기능 보기',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['내 것', '파일은 계속 내 것입니다'],
      ['비공개', '키는 내 기기에 보관됩니다'],
      ['어디서나', '내 클라우드로 동기화됩니다'],
    ],
    sheepAlt: '헤드폰을 쓴 bae 양',
    noteSynced: '동기화됨',
    noteSyncedSub: '기기 사이에서',
    noteCloud: '내 클라우드',
    noteCloudSub: '서버 없음',
    syncEyebrow: '암호화된 동기화',
    syncTitle: ['나만의 라이브러리를', '모든 기기에서'],
    syncText:
      '내 기기는 내가 선택한 클라우드 저장소를 통해 동기화됩니다. 실행할 서버가 없습니다. 모든 것은 기기를 떠나기 전에 암호화됩니다. 읽을 수 있는 사람은 나뿐입니다.',
    desktop: '내 데스크톱',
    phone: '내 휴대폰',
    holdsKeys: '키를 보관',
    encrypted: '암호화됨',
    cloudStorage: '내 클라우드 저장소',
    encryptionLink: '암호화 방식 보기 →',
    libraryEyebrow: '라이브러리',
    libraryTitle: ['내 전체 컬렉션을', '릴리스 단위로 정확하게'],
    libraryText: '소유한 앨범을 하나의 라이브러리에 넣고 어디든 가져가세요.',
    cards: [
      ['폴더가 아니라 릴리스', '음악 폴더를 지정하면 bae가 각 폴더를 올바른 릴리스와 맞추도록 돕습니다. 같은 앨범의 다른 릴리스는 함께 묶입니다.', ['메타데이터 소스', '커버 아트', '묶인 릴리스']],
      ['전체 라이브러리, 모든 기기', '클라우드가 라이브러리를 보관합니다. 모든 기기에서 전체를 볼 수 있습니다. 트랙은 재생할 때 내려받고, 고정한 릴리스는 오프라인에서 재생됩니다.', ['휴대폰과 데스크톱', '오프라인 고정']],
      ['재생 세부 정보', '앨범은 재생 세부 정보를 유지합니다. 레코드와 카세트 녹음은 면이 바뀔 때 멈춥니다. CUE 시트는 CD 프리갭 재생을 가능하게 합니다.', ['CD 프리갭', '레코드 면 전환']],
      ['내가 제어', '음악은 계속 내 것입니다. 파일은 내가 둔 위치에 남습니다. 라이브러리는 내가 제어합니다.', ['내 파일', '잠금 없음']],
    ],
    engineEyebrow: '내부 구조',
    engineTitle: ['모든 플랫폼의', '네이티브 앱'],
    engineText:
      '각 앱은 해당 플랫폼의 네이티브 앱입니다. 모두 같은 Rust 코어로 라이브러리, 재생, 동기화, 암호화를 처리합니다.',
    rustCore: 'Rust 코어',
    rustCoreSub: '라이브러리 · 재생 · 동기화 · 암호화',
    sharedRustCore: '공유 Rust 코어',
    deps: [
      ['FFmpeg', '오디오 디코딩'],
      ['SQLite', '라이브러리 데이터베이스'],
      ['클라우드 저장소', '동기화 백엔드'],
    ],
    playbackEyebrow: '재생',
    playbackTitle: '내 파일을 중심으로',
    minis: [
      ['원본 품질', '파일은 그대로 유지됩니다. 재생은 원본 파일을 사용합니다.'],
      ['로컬 우선', '파일을 바꾸지 않고 폴더를 릴리스와 맞춥니다. 동기화가 필요할 때 클라우드 저장소를 추가하세요.'],
      ['오프라인 재생', '릴리스를 기기에 고정하세요. 오프라인에서도 계속 재생됩니다.'],
    ],
    endAria: '컬렉션을 집으로 가져오기',
    endPrefix: '컬렉션을',
    endFallback: '집으로.',
    endText: '비공개, 오픈 소스, 개발 중입니다. 추가한 모든 것은 계속 내 것입니다.',
    readArchitecture: '아키텍처 읽기',
    footerStatus: '비공개 및 오픈 소스',
    cycleWords: ['집으로', '어디서나', '함께', '오프라인', '내 제어 아래'],
  },
  'zh-Hans': {
    metaDescription:
      'bae 是一个音乐资料库，用来管理你拥有的专辑，可在手机和桌面端使用，并通过你已有的云存储同步。bae 仍处于 1.0 之前，不适合普通使用。',
    title: 'bae：你的音乐，随处可用',
    navDocs: '文档',
    navDownload: '下载',
    languageLabel: '语言',
    heroTitle: ['你的音乐', '随处可用'],
    heroText:
      '你的全部音乐，都在手机和桌面端。通过你的云存储在设备间同步，无需运行服务器。',
    heroBold: '私有，属于你。',
    statusLabel: 'bae 开发状态',
    statusKicker: 'Pre-1.0',
    statusTitle: '不适合普通使用。',
    statusText: '仅限测试构建。数据和同步格式会在没有迁移的情况下更改。',
    downloadMac: '下载 macOS 版',
    seeFeatures: '查看功能',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['属于你', '你的文件仍然属于你'],
      ['私有', '你的设备保存密钥'],
      ['随处可用', '通过你的云同步'],
    ],
    sheepAlt: '戴着耳机的 bae 羊',
    noteSynced: '已同步',
    noteSyncedSub: '跨你的设备',
    noteCloud: '你的云',
    noteCloudSub: '无服务器',
    syncEyebrow: '加密同步',
    syncTitle: ['只属于你，位于', '每台设备'],
    syncText:
      '你的设备通过你选择的云存储同步。无需运行服务器。所有内容离开设备前都会加密。只有你能读取。',
    desktop: '你的桌面端',
    phone: '你的手机',
    holdsKeys: '保存密钥',
    encrypted: '已加密',
    cloudStorage: '你的云存储',
    encryptionLink: '查看加密如何工作 →',
    libraryEyebrow: '资料库',
    libraryTitle: ['你的整个收藏，', '按发行版本准确整理'],
    libraryText: '把你拥有的专辑放进一个资料库，然后带到任何地方。',
    cards: [
      ['发行版本，而不是文件夹', '把音乐文件夹交给 bae，bae 会帮助把每个文件夹匹配到正确的发行版本。同一张专辑的不同发行版本会分组显示。', ['元数据来源', '封面图', '发行版本分组']],
      ['整个资料库，每台设备', '你的云保存资料库。每台设备都能看到全部内容。曲目播放时下载；固定的发行版本可离线播放。', ['手机和桌面端', '离线固定']],
      ['播放细节', '专辑保留播放细节：黑胶和磁带录音会在换面时暂停。CUE 表可用于播放 CD pregap。', ['CD pregap', '黑胶换面']],
      ['由你控制', '你的音乐仍然属于你。你的文件留在你放置的位置。你控制自己的资料库。', ['你的文件', '无锁定']],
    ],
    engineEyebrow: '内部结构',
    engineTitle: ['每个平台上的', '原生应用'],
    engineText:
      '每个应用都是对应平台的原生应用。它们都使用同一个 Rust 核心来处理资料库、播放、同步和加密。',
    rustCore: 'Rust 核心',
    rustCoreSub: '资料库 · 播放 · 同步 · 加密',
    sharedRustCore: '共享 Rust 核心',
    deps: [
      ['FFmpeg', '音频解码'],
      ['SQLite', '资料库数据库'],
      ['云存储', '同步后端'],
    ],
    playbackEyebrow: '播放',
    playbackTitle: '围绕你的文件构建',
    minis: [
      ['原始质量', '你的文件会保持原样。播放使用原始文件。'],
      ['本地优先', '在不更改文件的情况下把文件夹匹配到发行版本。需要同步时再添加云存储。'],
      ['离线播放', '把发行版本固定到设备。它们会继续离线播放。'],
    ],
    endAria: '把你的收藏带回家',
    endPrefix: '把你的收藏带到',
    endFallback: '身边。',
    endText: '私有、开源，并处于积极开发中。你添加的一切仍然属于你。',
    readArchitecture: '阅读架构',
    footerStatus: '私有且开源',
    cycleWords: ['身边', '任何地方', '一起', '离线', '由你控制'],
  },
  ar: {
    metaDescription:
      'bae مكتبة موسيقى للألبومات التي تملكها، على الهاتف وسطح المكتب، وتتم مزامنتها عبر التخزين السحابي الذي لديك بالفعل. bae قبل الإصدار 1.0 وليست جاهزة للاستخدام العام.',
    title: 'bae: موسيقاك في كل مكان',
    navDocs: 'المستندات',
    navDownload: 'تنزيل',
    languageLabel: 'اللغة',
    heroTitle: ['موسيقاك', 'في كل مكان'],
    heroText:
      'كل موسيقاك على الهاتف وسطح المكتب. استمع عبر أجهزتك، مع مزامنة من خلال تخزينك السحابي. لا يوجد خادم تحتاج إلى تشغيله.',
    heroBold: 'خاصة، وملكك.',
    statusLabel: 'حالة تطوير bae',
    statusKicker: 'قبل 1.0',
    statusTitle: 'ليست جاهزة للاستخدام العام.',
    statusText: 'إصدارات اختبار فقط. ستتغير صيغ البيانات والمزامنة بلا ترحيل.',
    downloadMac: 'تنزيل لـ macOS',
    seeFeatures: 'عرض الميزات',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['ملكك', 'ملفاتك تبقى ملكك'],
      ['خاصة', 'أجهزتك تحتفظ بالمفاتيح'],
      ['في كل مكان', 'تتم المزامنة عبر سحابتك'],
    ],
    sheepAlt: 'خروف bae يرتدي سماعات',
    noteSynced: 'متزامنة',
    noteSyncedSub: 'عبر أجهزتك',
    noteCloud: 'سحابتك',
    noteCloudSub: 'بلا خادم',
    syncEyebrow: 'مزامنة مشفرة',
    syncTitle: ['لك وحدك، على', 'كل جهاز'],
    syncText:
      'تتزامن أجهزتك عبر التخزين السحابي الذي تختاره. لا يوجد خادم لتشغيله. كل شيء يُشفّر قبل أن يغادر جهازك. أنت فقط تستطيع قراءته.',
    desktop: 'سطح مكتبك',
    phone: 'هاتفك',
    holdsKeys: 'يحتفظ بالمفاتيح',
    encrypted: 'مشفر',
    cloudStorage: 'تخزينك السحابي',
    encryptionLink: 'اطلع على طريقة عمل التشفير →',
    libraryEyebrow: 'المكتبة',
    libraryTitle: ['مجموعتك كاملة،', 'بدقة حسب الإصدار'],
    libraryText: 'اجمع الألبومات التي تملكها في مكتبة واحدة، ثم خذها معك.',
    cards: [
      ['إصدارات، لا مجلدات', 'وجّه bae إلى مجلدات الموسيقى، وسيساعدك على مطابقة كل واحد بالإصدار الصحيح. تُجمع الإصدارات المختلفة للألبوم نفسه معاً.', ['مصادر البيانات الوصفية', 'الغلاف', 'إصدارات مجمعة']],
      ['المكتبة كاملة، على كل جهاز', 'سحابتك تحتفظ بالمكتبة. كل جهاز يراها كاملة. تُنزّل المقاطع عند تشغيلها؛ والمثبتة تعمل دون اتصال.', ['الهاتف وسطح المكتب', 'تثبيت دون اتصال']],
      ['تفاصيل التشغيل', 'تحتفظ الألبومات بتفاصيل التشغيل: تسجيلات الفينيل والكاسيت تتوقف عند فواصل الجوانب. تتيح ملفات CUE تشغيل pregaps الخاصة بالأقراص المضغوطة.', ['CD pregaps', 'فواصل جوانب الفينيل']],
      ['أنت تتحكم بها', 'موسيقاك تبقى ملكك. ملفاتك تبقى حيث وضعتها. أنت تتحكم بمكتبتك.', ['ملفاتك', 'لا حبس']],
    ],
    engineEyebrow: 'تحت السطح',
    engineTitle: ['تطبيقات أصلية على', 'كل منصة'],
    engineText:
      'كل تطبيق أصلي لمنصته. كلها تستخدم نواة Rust نفسها للمكتبة والتشغيل والمزامنة والتشفير.',
    rustCore: 'نواة Rust',
    rustCoreSub: 'مكتبة · تشغيل · مزامنة · تشفير',
    sharedRustCore: 'نواة Rust مشتركة',
    deps: [
      ['FFmpeg', 'فك ترميز الصوت'],
      ['SQLite', 'قاعدة بيانات المكتبة'],
      ['التخزين السحابي', 'خلفية المزامنة'],
    ],
    playbackEyebrow: 'التشغيل',
    playbackTitle: 'مبنية حول ملفاتك',
    minis: [
      ['الجودة الأصلية', 'تبقى ملفاتك كما هي تماماً. يستخدم التشغيل الملفات الأصلية.'],
      ['محلي أولاً', 'طابق المجلدات مع الإصدارات دون تغيير ملفاتك. أضف التخزين السحابي عندما تريد المزامنة.'],
      ['تعمل دون اتصال', 'ثبّت الإصدارات على جهازك. ستستمر في التشغيل دون اتصال.'],
    ],
    endAria: 'أعد مجموعتك إلى بيتك',
    endPrefix: 'أعد مجموعتك',
    endFallback: 'إلى البيت.',
    endText: 'خاصة ومفتوحة المصدر وقيد التطوير النشط. كل ما تضيفه يبقى ملكك.',
    readArchitecture: 'اقرأ البنية',
    footerStatus: 'خاصة ومفتوحة المصدر',
    cycleWords: ['إلى البيت', 'في كل مكان', 'معاً', 'دون اتصال', 'تحت سيطرتك'],
  },
  he: {
    metaDescription:
      'bae היא ספריית מוזיקה לאלבומים שבבעלותך, בטלפון ובשולחן העבודה, עם סנכרון דרך אחסון הענן שכבר יש לך. bae לפני גרסה 1.0 ואינה מוכנה לשימוש כללי.',
    title: 'bae: המוזיקה שלך בכל מקום',
    navDocs: 'מסמכים',
    navDownload: 'הורדה',
    languageLabel: 'שפה',
    heroTitle: ['המוזיקה שלך', 'בכל מקום'],
    heroText:
      'כל המוזיקה שלך, בטלפון ובשולחן העבודה. האזנה בין מכשירים, מסונכרנת דרך אחסון הענן שלך. אין שרת שצריך להפעיל.',
    heroBold: 'פרטית, שלך.',
    statusLabel: 'מצב הפיתוח של bae',
    statusKicker: 'לפני 1.0',
    statusTitle: 'לא מוכנה לשימוש כללי.',
    statusText: 'גרסאות בדיקה בלבד. מבני הנתונים והסנכרון ישתנו ללא הגירה.',
    downloadMac: 'הורדה ל-macOS',
    seeFeatures: 'הצגת תכונות',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['שלך', 'הקבצים שלך נשארים שלך'],
      ['פרטית', 'המכשירים שלך מחזיקים את המפתחות'],
      ['בכל מקום', 'מסונכרנת דרך הענן שלך'],
    ],
    sheepAlt: 'הכבשה של bae עם אוזניות',
    noteSynced: 'מסונכרנת',
    noteSyncedSub: 'בין המכשירים שלך',
    noteCloud: 'הענן שלך',
    noteCloudSub: 'בלי שרת',
    syncEyebrow: 'סנכרון מוצפן',
    syncTitle: ['שלך בלבד, על', 'כל מכשיר'],
    syncText:
      'המכשירים שלך מסתנכרנים דרך אחסון הענן שתבחר. אין שרת להריץ. הכל מוצפן לפני שהוא עוזב את המכשיר. רק לך יש אפשרות לקרוא.',
    desktop: 'שולחן העבודה שלך',
    phone: 'הטלפון שלך',
    holdsKeys: 'מחזיק את המפתחות',
    encrypted: 'מוצפן',
    cloudStorage: 'אחסון הענן שלך',
    encryptionLink: 'איך ההצפנה עובדת →',
    libraryEyebrow: 'ספרייה',
    libraryTitle: ['כל האוסף שלך,', 'מדויק לפי מהדורה'],
    libraryText: 'אסוף את האלבומים שבבעלותך בספרייה אחת וקח אותם לכל מקום.',
    cards: [
      ['מהדורות, לא תיקיות', 'כוון את bae לתיקיות המוזיקה שלך, והיא תעזור להתאים כל אחת למהדורה הנכונה. מהדורות שונות של אותו אלבום מקובצות יחד.', ['מקורות מטא-נתונים', 'עטיפות', 'מהדורות מקובצות']],
      ['כל הספרייה, בכל מכשיר', 'הענן שלך מחזיק את הספרייה. כל מכשיר רואה את כולה. רצועות יורדות בזמן ההשמעה; נעיצות פועלות במצב לא מקוון.', ['טלפון ושולחן עבודה', 'נעיצות לא מקוונות']],
      ['פרטי השמעה', 'אלבומים שומרים את פרטי ההשמעה שלהם: הקלטות ויניל וקלטות נעצרות במעברי צד. גיליונות CUE מאפשרים להשמיע pregaps מתקליטורים.', ['CD pregaps', 'מעברי צד ויניל']],
      ['השליטה אצלך', 'המוזיקה שלך נשארת שלך. הקבצים שלך נשארים במקום שבו שמת אותם. השליטה בספרייה אצלך.', ['הקבצים שלך', 'בלי נעילה']],
    ],
    engineEyebrow: 'מתחת לפני השטח',
    engineTitle: ['אפליקציות מקוריות על', 'כל פלטפורמה'],
    engineText:
      'כל אפליקציה מקורית לפלטפורמה שלה. כולן משתמשות באותה ליבת Rust לספרייה, השמעה, סנכרון והצפנה.',
    rustCore: 'ליבת Rust',
    rustCoreSub: 'ספרייה · השמעה · סנכרון · הצפנה',
    sharedRustCore: 'ליבת Rust משותפת',
    deps: [
      ['FFmpeg', 'פענוח שמע'],
      ['SQLite', 'מסד נתוני ספרייה'],
      ['אחסון ענן', 'גב סנכרון'],
    ],
    playbackEyebrow: 'השמעה',
    playbackTitle: 'בנויה סביב הקבצים שלך',
    minis: [
      ['איכות מקורית', 'הקבצים שלך נשארים בדיוק כפי שהם. ההשמעה משתמשת בקבצים המקוריים.'],
      ['מקומי תחילה', 'התאם תיקיות למהדורות בלי לשנות את הקבצים. הוסף אחסון ענן כשתרצה סנכרון.'],
      ['פועלת לא מקוון', 'נעץ מהדורות למכשיר. הן ממשיכות להתנגן לא מקוון.'],
    ],
    endAria: 'הבא את האוסף שלך הביתה',
    endPrefix: 'הבא את האוסף שלך',
    endFallback: 'הביתה.',
    endText: 'פרטית, קוד פתוח, ובפיתוח פעיל. כל מה שתוסיף נשאר שלך.',
    readArchitecture: 'קריאת הארכיטקטורה',
    footerStatus: 'פרטית וקוד פתוח',
    cycleWords: ['הביתה', 'לכל מקום', 'יחד', 'לא מקוון', 'בשליטתך'],
  },
  uk: {
    metaDescription:
      'bae — музична бібліотека для альбомів, які вам належать, на телефоні й комп’ютері, із синхронізацією через ваше хмарне сховище. bae ще до версії 1.0 і не готова для загального використання.',
    title: 'bae: ваша музика всюди',
    navDocs: 'Документи',
    navDownload: 'Завантажити',
    languageLabel: 'Мова',
    heroTitle: ['Ваша музика', 'всюди'],
    heroText:
      'Уся ваша музика на телефоні й комп’ютері. Слухайте на різних пристроях, синхронізованих через ваше хмарне сховище. Без сервера, який треба підтримувати.',
    heroBold: 'Приватна, ваша.',
    statusLabel: 'стан розробки bae',
    statusKicker: 'До 1.0',
    statusTitle: 'Не готова для загального використання.',
    statusText: 'Лише тестові збірки. Формати даних і синхронізації змінюватимуться без міграції.',
    downloadMac: 'Завантажити для macOS',
    seeFeatures: 'Переглянути функції',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Ваша', 'ваші файли залишаються вашими'],
      ['Приватна', 'ключі зберігаються на ваших пристроях'],
      ['Всюди', 'синхронізується через вашу хмару'],
    ],
    sheepAlt: 'вівця bae у навушниках',
    noteSynced: 'Синхронізовано',
    noteSyncedSub: 'між вашими пристроями',
    noteCloud: 'Ваша хмара',
    noteCloudSub: 'без сервера',
    syncEyebrow: 'Зашифрована синхронізація',
    syncTitle: ['Лише ваша, на', 'кожному пристрої'],
    syncText:
      'Ваші пристрої синхронізуються через хмарне сховище, яке ви обираєте. Немає сервера для запуску. Усе шифрується до виходу з пристрою. Прочитати можете тільки ви.',
    desktop: 'Ваш комп’ютер',
    phone: 'Ваш телефон',
    holdsKeys: 'зберігає ключі',
    encrypted: 'зашифровано',
    cloudStorage: 'Ваше хмарне сховище',
    encryptionLink: 'Як працює шифрування →',
    libraryEyebrow: 'Бібліотека',
    libraryTitle: ['Уся ваша колекція,', 'точна до релізу'],
    libraryText: 'Зберіть альбоми, які вам належать, в одну бібліотеку й беріть їх із собою.',
    cards: [
      ['Релізи, а не папки', 'Вкажіть bae папки з музикою, і вона допоможе зіставити кожну з правильним релізом. Різні релізи одного альбому групуються разом.', ['Джерела метаданих', 'Обкладинки', 'Згруповані релізи']],
      ['Уся бібліотека, кожен пристрій', 'Ваша хмара зберігає бібліотеку. Кожен пристрій бачить її повністю. Треки завантажуються під час відтворення; закріплені релізи працюють офлайн.', ['Телефон і комп’ютер', 'Офлайн-закріплення']],
      ['Деталі відтворення', 'Альбоми зберігають деталі відтворення: записи з вінілу й касет зупиняються на переходах сторін. CUE-файли дозволяють відтворювати CD pregaps.', ['CD pregaps', 'Переходи сторін вінілу']],
      ['Ви контролюєте', 'Ваша музика залишається вашою. Ваші файли лишаються там, де ви їх розмістили. Бібліотека під вашим контролем.', ['Ваші файли', 'Без прив’язки']],
    ],
    engineEyebrow: 'Під капотом',
    engineTitle: ['Нативні застосунки на', 'кожній платформі'],
    engineText:
      'Кожен застосунок нативний для своєї платформи. Усі використовують те саме ядро Rust для бібліотеки, відтворення, синхронізації й шифрування.',
    rustCore: 'Ядро Rust',
    rustCoreSub: 'бібліотека · відтворення · синхронізація · шифрування',
    sharedRustCore: 'спільне ядро Rust',
    deps: [
      ['FFmpeg', 'декодування аудіо'],
      ['SQLite', 'база даних бібліотеки'],
      ['Хмарне сховище', 'бекенд синхронізації'],
    ],
    playbackEyebrow: 'Відтворення',
    playbackTitle: 'Побудовано навколо ваших файлів',
    minis: [
      ['Оригінальна якість', 'Ваші файли залишаються саме такими, як є. Відтворення використовує оригінальні файли.'],
      ['Спершу локально', 'Зіставляйте папки з релізами без зміни файлів. Додайте хмарне сховище, коли захочете синхронізацію.'],
      ['Працює офлайн', 'Закріплюйте релізи на пристрої. Вони продовжують відтворюватися офлайн.'],
    ],
    endAria: 'Поверніть колекцію додому',
    endPrefix: 'Поверніть колекцію',
    endFallback: 'додому.',
    endText: 'Приватна, з відкритим кодом і в активній розробці. Усе, що ви додаєте, залишається вашим.',
    readArchitecture: 'Читати архітектуру',
    footerStatus: 'приватна й з відкритим кодом',
    cycleWords: ['додому', 'всюди', 'разом', 'офлайн', 'під контролем'],
  },
  bg: {
    metaDescription:
      'bae е музикална библиотека за албумите, които притежавате, на телефона и компютъра, синхронизирана чрез облачното хранилище, което вече имате. bae е преди версия 1.0 и не е готова за обща употреба.',
    title: 'bae: вашата музика навсякъде',
    navDocs: 'Документи',
    navDownload: 'Изтегляне',
    languageLabel: 'Език',
    heroTitle: ['Вашата музика', 'навсякъде'],
    heroText:
      'Цялата ви музика на телефона и компютъра. Слушайте на различни устройства, синхронизирани чрез вашето облачно хранилище. Няма сървър за поддръжка.',
    heroBold: 'Лична, ваша.',
    statusLabel: 'състояние на разработката на bae',
    statusKicker: 'Преди 1.0',
    statusTitle: 'Не е готова за обща употреба.',
    statusText: 'Само тестови сборки. Форматите за данни и синхронизация ще се променят без миграция.',
    downloadMac: 'Изтегляне за macOS',
    seeFeatures: 'Вижте функциите',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Ваша', 'файловете ви остават ваши'],
      ['Лична', 'устройствата ви пазят ключовете'],
      ['Навсякъде', 'синхронизирана чрез вашия облак'],
    ],
    sheepAlt: 'овцата на bae със слушалки',
    noteSynced: 'Синхронизирана',
    noteSyncedSub: 'между вашите устройства',
    noteCloud: 'Вашият облак',
    noteCloudSub: 'без сървър',
    syncEyebrow: 'Шифрована синхронизация',
    syncTitle: ['Само ваша, на', 'всяко устройство'],
    syncText:
      'Устройствата ви се синхронизират чрез облачното хранилище, което изберете. Няма сървър за стартиране. Всичко се шифрова, преди да напусне устройството. Само вие можете да го четете.',
    desktop: 'Вашият компютър',
    phone: 'Вашият телефон',
    holdsKeys: 'пази ключовете',
    encrypted: 'шифровано',
    cloudStorage: 'Вашето облачно хранилище',
    encryptionLink: 'Как работи шифроването →',
    libraryEyebrow: 'Библиотека',
    libraryTitle: ['Цялата ви колекция,', 'точна по издание'],
    libraryText: 'Съберете албумите, които притежавате, в една библиотека и ги носете със себе си.',
    cards: [
      ['Издания, не папки', 'Посочете на bae вашите музикални папки и тя ще помогне да съпостави всяка с правилното издание. Различните издания на един и същи албум се групират заедно.', ['Източници на метаданни', 'Обложки', 'Групирани издания']],
      ['Цялата библиотека, всяко устройство', 'Вашият облак държи библиотеката. Всяко устройство я вижда цялата. Песните се изтеглят при възпроизвеждане; фиксираните издания работят офлайн.', ['Телефон и компютър', 'Офлайн фиксиране']],
      ['Детайли за възпроизвеждане', 'Албумите пазят детайлите си за възпроизвеждане: записи от винил и касета спират при смяна на страна. CUE файловете позволяват CD pregaps.', ['CD pregaps', 'Смяна на страна на винил']],
      ['Вие контролирате', 'Музиката ви остава ваша. Файловете ви остават там, където ги поставите. Вие контролирате библиотеката си.', ['Вашите файлове', 'Без заключване']],
    ],
    engineEyebrow: 'Под капака',
    engineTitle: ['Нативни приложения на', 'всяка платформа'],
    engineText:
      'Всяко приложение е нативно за своята платформа. Всички използват едно и също Rust ядро за библиотека, възпроизвеждане, синхронизация и шифроване.',
    rustCore: 'Rust ядро',
    rustCoreSub: 'библиотека · възпроизвеждане · синхронизация · шифроване',
    sharedRustCore: 'споделено Rust ядро',
    deps: [
      ['FFmpeg', 'аудио декодиране'],
      ['SQLite', 'база данни на библиотеката'],
      ['Облачно хранилище', 'бекенд за синхронизация'],
    ],
    playbackEyebrow: 'Възпроизвеждане',
    playbackTitle: 'Изградено около вашите файлове',
    minis: [
      ['Оригинално качество', 'Файловете ви остават точно такива, каквито са. Възпроизвеждането използва оригиналните файлове.'],
      ['Първо локално', 'Съпоставяйте папки с издания, без да променяте файловете. Добавете облачно хранилище, когато искате синхронизация.'],
      ['Работи офлайн', 'Фиксирайте издания на устройството. Те продължават да се възпроизвеждат офлайн.'],
    ],
    endAria: 'Върнете колекцията си у дома',
    endPrefix: 'Върнете колекцията си',
    endFallback: 'у дома.',
    endText: 'Лична, с отворен код и в активна разработка. Всичко, което добавите, остава ваше.',
    readArchitecture: 'Прочетете архитектурата',
    footerStatus: 'лична и с отворен код',
    cycleWords: ['у дома', 'навсякъде', 'заедно', 'офлайн', 'под контрол'],
  },
  pl: {
    metaDescription:
      'bae to biblioteka muzyczna dla albumów, które posiadasz, na telefonie i komputerze, synchronizowana przez chmurę, którą już masz. bae jest przed wersją 1.0 i nie jest gotowa do ogólnego użytku.',
    title: 'bae: twoja muzyka wszędzie',
    navDocs: 'Dokumenty',
    navDownload: 'Pobierz',
    languageLabel: 'Język',
    heroTitle: ['Twoja muzyka', 'wszędzie'],
    heroText:
      'Cała twoja muzyka na telefonie i komputerze. Słuchaj na różnych urządzeniach, synchronizowanych przez twoją chmurę. Bez serwera do utrzymywania.',
    heroBold: 'Prywatna, twoja.',
    statusLabel: 'stan rozwoju bae',
    statusKicker: 'Przed 1.0',
    statusTitle: 'Nie jest gotowa do ogólnego użytku.',
    statusText: 'Tylko kompilacje testowe. Formaty danych i synchronizacji będą się zmieniać bez migracji.',
    downloadMac: 'Pobierz dla macOS',
    seeFeatures: 'Zobacz funkcje',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Twoja', 'twoje pliki zostają twoje'],
      ['Prywatna', 'twoje urządzenia trzymają klucze'],
      ['Wszędzie', 'synchronizowana przez twoją chmurę'],
    ],
    sheepAlt: 'owca bae w słuchawkach',
    noteSynced: 'Zsynchronizowana',
    noteSyncedSub: 'między twoimi urządzeniami',
    noteCloud: 'Twoja chmura',
    noteCloudSub: 'bez serwera',
    syncEyebrow: 'Szyfrowana synchronizacja',
    syncTitle: ['Tylko twoja, na', 'każdym urządzeniu'],
    syncText:
      'Twoje urządzenia synchronizują się przez wybraną chmurę. Bez serwera do uruchamiania. Wszystko jest szyfrowane przed opuszczeniem urządzenia. Tylko ty możesz to odczytać.',
    desktop: 'Twój komputer',
    phone: 'Twój telefon',
    holdsKeys: 'trzyma klucze',
    encrypted: 'zaszyfrowane',
    cloudStorage: 'Twoja chmura',
    encryptionLink: 'Zobacz, jak działa szyfrowanie →',
    libraryEyebrow: 'Biblioteka',
    libraryTitle: ['Cała kolekcja,', 'dokładna co do wydania'],
    libraryText: 'Zbierz albumy, które posiadasz, w jednej bibliotece i zabierz je ze sobą.',
    cards: [
      ['Wydania, nie foldery', 'Wskaż foldery z muzyką, a bae pomoże dopasować każdy z nich do właściwego wydania. Różne wydania tego samego albumu są grupowane razem.', ['Źródła metadanych', 'Okładki', 'Wydania pogrupowane']],
      ['Cała biblioteka, każde urządzenie', 'Twoja chmura trzyma bibliotekę. Każde urządzenie widzi całość. Utwory pobierają się podczas odtwarzania; przypięte wydania grają offline.', ['Telefon i komputer', 'Przypięcia offline']],
      ['Szczegóły odtwarzania', 'Albumy zachowują szczegóły odtwarzania: nagrania z winylu i kaset zatrzymują się przy zmianie strony. Arkusze CUE pozwalają odtwarzać CD pregaps.', ['CD pregaps', 'Zmiany strony winylu']],
      ['Ty kontrolujesz', 'Twoja muzyka zostaje twoja. Twoje pliki zostają tam, gdzie je umieścisz. Ty kontrolujesz bibliotekę.', ['Twoje pliki', 'Bez zamknięcia']],
    ],
    engineEyebrow: 'Pod spodem',
    engineTitle: ['Natywne aplikacje na', 'każdej platformie'],
    engineText:
      'Każda aplikacja jest natywna dla swojej platformy. Wszystkie używają tego samego rdzenia Rust do biblioteki, odtwarzania, synchronizacji i szyfrowania.',
    rustCore: 'Rdzeń Rust',
    rustCoreSub: 'biblioteka · odtwarzanie · synchronizacja · szyfrowanie',
    sharedRustCore: 'wspólny rdzeń Rust',
    deps: [
      ['FFmpeg', 'dekodowanie audio'],
      ['SQLite', 'baza danych biblioteki'],
      ['Chmura', 'backend synchronizacji'],
    ],
    playbackEyebrow: 'Odtwarzanie',
    playbackTitle: 'Zbudowana wokół twoich plików',
    minis: [
      ['Oryginalna jakość', 'Twoje pliki pozostają dokładnie takie, jakie są. Odtwarzanie używa oryginalnych plików.'],
      ['Najpierw lokalnie', 'Dopasuj foldery do wydań bez zmieniania plików. Dodaj chmurę, gdy chcesz synchronizacji.'],
      ['Działa offline', 'Przypnij wydania do urządzenia. Będą dalej grać offline.'],
    ],
    endAria: 'Przynieś kolekcję do domu',
    endPrefix: 'Przynieś kolekcję',
    endFallback: 'do domu.',
    endText: 'Prywatna, open source i aktywnie rozwijana. Wszystko, co dodasz, pozostaje twoje.',
    readArchitecture: 'Czytaj architekturę',
    footerStatus: 'prywatna i open source',
    cycleWords: ['do domu', 'wszędzie', 'razem', 'offline', 'pod kontrolą'],
  },
  cs: {
    metaDescription:
      'bae je hudební knihovna pro alba, která vlastníte, v telefonu i na počítači, synchronizovaná přes cloudové úložiště, které už máte. bae je před verzí 1.0 a není připravena pro obecné použití.',
    title: 'bae: vaše hudba všude',
    navDocs: 'Dokumentace',
    navDownload: 'Stáhnout',
    languageLabel: 'Jazyk',
    heroTitle: ['Vaše hudba', 'všude'],
    heroText:
      'Veškerá vaše hudba v telefonu i na počítači. Poslouchejte na více zařízeních, synchronizovaných přes vaše cloudové úložiště. Žádný server k provozu.',
    heroBold: 'Soukromá, vaše.',
    statusLabel: 'stav vývoje bae',
    statusKicker: 'Před 1.0',
    statusTitle: 'Není připravena pro obecné použití.',
    statusText: 'Pouze testovací buildy. Formáty dat a synchronizace se budou měnit bez migrace.',
    downloadMac: 'Stáhnout pro macOS',
    seeFeatures: 'Zobrazit funkce',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Vaše', 'vaše soubory zůstávají vaše'],
      ['Soukromá', 'vaše zařízení drží klíče'],
      ['Všude', 'synchronizovaná přes váš cloud'],
    ],
    sheepAlt: 'ovce bae se sluchátky',
    noteSynced: 'Synchronizováno',
    noteSyncedSub: 'napříč vašimi zařízeními',
    noteCloud: 'Váš cloud',
    noteCloudSub: 'bez serveru',
    syncEyebrow: 'Šifrovaná synchronizace',
    syncTitle: ['Jen vaše, na', 'každém zařízení'],
    syncText:
      'Vaše zařízení se synchronizují přes cloudové úložiště, které si vyberete. Žádný server k provozu. Vše je zašifrováno před opuštěním zařízení. Číst to můžete jen vy.',
    desktop: 'Váš počítač',
    phone: 'Váš telefon',
    holdsKeys: 'drží klíče',
    encrypted: 'šifrováno',
    cloudStorage: 'Vaše cloudové úložiště',
    encryptionLink: 'Jak funguje šifrování →',
    libraryEyebrow: 'Knihovna',
    libraryTitle: ['Celá vaše sbírka,', 'přesná podle vydání'],
    libraryText: 'Dejte alba, která vlastníte, do jedné knihovny a vezměte je kamkoli.',
    cards: [
      ['Vydání, ne složky', 'Namiřte bae na své hudební složky a pomůže každou přiřadit ke správnému vydání. Různá vydání stejného alba se seskupí.', ['Zdroje metadat', 'Obaly', 'Seskupená vydání']],
      ['Celá knihovna, každé zařízení', 'Váš cloud drží knihovnu. Každé zařízení ji vidí celou. Skladby se stahují při přehrávání; připnutá vydání hrají offline.', ['Telefon a počítač', 'Offline připnutí']],
      ['Detaily přehrávání', 'Alba si drží detaily přehrávání: nahrávky z vinylu a kazet se pozastaví na přechodech stran. CUE soubory umožňují přehrávat CD pregaps.', ['CD pregaps', 'Přechody stran vinylu']],
      ['Máte kontrolu', 'Vaše hudba zůstává vaše. Vaše soubory zůstávají tam, kam je dáte. Knihovnu ovládáte vy.', ['Vaše soubory', 'Bez uzamčení']],
    ],
    engineEyebrow: 'Pod kapotou',
    engineTitle: ['Nativní aplikace na', 'každé platformě'],
    engineText:
      'Každá aplikace je nativní pro svou platformu. Všechny používají stejné jádro Rust pro knihovnu, přehrávání, synchronizaci a šifrování.',
    rustCore: 'Jádro Rust',
    rustCoreSub: 'knihovna · přehrávání · synchronizace · šifrování',
    sharedRustCore: 'sdílené jádro Rust',
    deps: [
      ['FFmpeg', 'dekódování zvuku'],
      ['SQLite', 'databáze knihovny'],
      ['Cloudové úložiště', 'backend synchronizace'],
    ],
    playbackEyebrow: 'Přehrávání',
    playbackTitle: 'Postaveno kolem vašich souborů',
    minis: [
      ['Původní kvalita', 'Vaše soubory zůstávají přesně takové, jaké jsou. Přehrávání používá původní soubory.'],
      ['Nejdřív lokálně', 'Přiřaďte složky k vydáním bez změny souborů. Cloudové úložiště přidejte, až budete chtít synchronizovat.'],
      ['Hraje offline', 'Připněte vydání do zařízení. Budou hrát dál offline.'],
    ],
    endAria: 'Přineste sbírku domů',
    endPrefix: 'Přineste sbírku',
    endFallback: 'domů.',
    endText: 'Soukromá, open source a v aktivním vývoji. Vše, co přidáte, zůstává vaše.',
    readArchitecture: 'Číst architekturu',
    footerStatus: 'soukromá a open source',
    cycleWords: ['domů', 'všude', 'společně', 'offline', 'pod kontrolou'],
  },
  hr: {
    metaDescription:
      'bae je glazbena biblioteka za albume koje posjedujete, na telefonu i računalu, sinkronizirana kroz oblak koji već imate. bae je prije verzije 1.0 i nije spremna za opću upotrebu.',
    title: 'bae: vaša glazba svugdje',
    navDocs: 'Dokumenti',
    navDownload: 'Preuzmi',
    languageLabel: 'Jezik',
    heroTitle: ['Vaša glazba', 'svugdje'],
    heroText:
      'Sva vaša glazba na telefonu i računalu. Slušajte na više uređaja, sinkronizirano kroz vašu pohranu u oblaku. Nema poslužitelja koji treba održavati.',
    heroBold: 'Privatna, vaša.',
    statusLabel: 'stanje razvoja bae',
    statusKicker: 'Prije 1.0',
    statusTitle: 'Nije spremna za opću upotrebu.',
    statusText: 'Samo testne gradnje. Formati podataka i sinkronizacije mijenjat će se bez migracije.',
    downloadMac: 'Preuzmi za macOS',
    seeFeatures: 'Pogledaj značajke',
    platformMeta: 'macOS · iOS · Windows · Android',
    trust: [
      ['Vaša', 'vaše datoteke ostaju vaše'],
      ['Privatna', 'vaši uređaji čuvaju ključeve'],
      ['Svugdje', 'sinkronizirana kroz vaš oblak'],
    ],
    sheepAlt: 'bae ovca sa slušalicama',
    noteSynced: 'Sinkronizirano',
    noteSyncedSub: 'među vašim uređajima',
    noteCloud: 'Vaš oblak',
    noteCloudSub: 'bez poslužitelja',
    syncEyebrow: 'Šifrirana sinkronizacija',
    syncTitle: ['Samo vaša, na', 'svakom uređaju'],
    syncText:
      'Vaši se uređaji sinkroniziraju kroz pohranu u oblaku koju odaberete. Nema poslužitelja za pokretanje. Sve se šifrira prije nego što napusti uređaj. Samo vi to možete čitati.',
    desktop: 'Vaše računalo',
    phone: 'Vaš telefon',
    holdsKeys: 'čuva ključeve',
    encrypted: 'šifrirano',
    cloudStorage: 'Vaša pohrana u oblaku',
    encryptionLink: 'Pogledajte kako šifriranje radi →',
    libraryEyebrow: 'Biblioteka',
    libraryTitle: ['Cijela vaša kolekcija,', 'točna po izdanju'],
    libraryText: 'Stavite albume koje posjedujete u jednu biblioteku i ponesite ih svugdje.',
    cards: [
      ['Izdanja, ne mape', 'Usmjerite bae na svoje glazbene mape i pomoći će povezati svaku s pravim izdanjem. Različita izdanja istog albuma grupiraju se zajedno.', ['Izvori metapodataka', 'Omoti', 'Grupirana izdanja']],
      ['Cijela biblioteka, svaki uređaj', 'Vaš oblak drži biblioteku. Svaki uređaj vidi sve. Pjesme se preuzimaju tijekom reprodukcije; prikvačena izdanja sviraju offline.', ['Telefon i računalo', 'Offline prikvačeno']],
      ['Detalji reprodukcije', 'Albumi čuvaju detalje reprodukcije: snimke vinila i kazeta pauziraju na prijelazima strana. CUE datoteke omogućuju reprodukciju CD pregaps.', ['CD pregaps', 'Prijelazi strana vinila']],
      ['Vi kontrolirate', 'Vaša glazba ostaje vaša. Vaše datoteke ostaju gdje ih stavite. Vi kontrolirate biblioteku.', ['Vaše datoteke', 'Bez zaključavanja']],
    ],
    engineEyebrow: 'Ispod površine',
    engineTitle: ['Nativne aplikacije na', 'svakoj platformi'],
    engineText:
      'Svaka aplikacija je nativna za svoju platformu. Sve koriste istu Rust jezgru za biblioteku, reprodukciju, sinkronizaciju i šifriranje.',
    rustCore: 'Rust jezgra',
    rustCoreSub: 'biblioteka · reprodukcija · sinkronizacija · šifriranje',
    sharedRustCore: 'zajednička Rust jezgra',
    deps: [
      ['FFmpeg', 'dekodiranje zvuka'],
      ['SQLite', 'baza podataka biblioteke'],
      ['Pohrana u oblaku', 'pozadina sinkronizacije'],
    ],
    playbackEyebrow: 'Reprodukcija',
    playbackTitle: 'Izgrađena oko vaših datoteka',
    minis: [
      ['Izvorna kvaliteta', 'Vaše datoteke ostaju točno kakve jesu. Reprodukcija koristi izvorne datoteke.'],
      ['Prvo lokalno', 'Povežite mape s izdanjima bez promjene datoteka. Dodajte pohranu u oblaku kad želite sinkronizaciju.'],
      ['Svira offline', 'Prikvačite izdanja na uređaj. Nastavljaju svirati offline.'],
    ],
    endAria: 'Donesite kolekciju kući',
    endPrefix: 'Donesite kolekciju',
    endFallback: 'kući.',
    endText: 'Privatna, otvorenog koda i u aktivnom razvoju. Sve što dodate ostaje vaše.',
    readArchitecture: 'Čitaj arhitekturu',
    footerStatus: 'privatna i otvorenog koda',
    cycleWords: ['kući', 'svugdje', 'zajedno', 'offline', 'pod kontrolom'],
  },
};








const addedSidebarTranslations = {
  sections: {
    gettingStarted: {
      it: "Primi passi",
      tr: "Başlarken",
      vi: "Bắt đầu",
      nl: "Aan de slag",
      hi: "शुरुआत",
      bn: "শুরু করা",
      ta: "தொடங்குதல்",
      te: "ప్రారంభం",
      mr: "सुरुवात",
      ur: "شروع کریں",
      gu: "શરૂઆત",
      kn: "ಪ್ರಾರಂಭ",
      ml: "ആരംഭിക്കുക",
      pa: "ਸ਼ੁਰੂਆਤ",
      th: "เริ่มต้น",
      "zh-Hant": "入門"
    },
    library: {
      it: "Libreria",
      tr: "Kitaplık",
      vi: "Thư viện",
      nl: "Bibliotheek",
      hi: "लाइब्रेरी",
      bn: "লাইব্রেরি",
      ta: "நூலகம்",
      te: "లైబ్రరీ",
      mr: "लायब्ररी",
      ur: "لائبریری",
      gu: "લાઇબ્રેરી",
      kn: "ಲೈಬ್ರರಿ",
      ml: "ലൈബ്രറി",
      pa: "ਲਾਇਬ੍ਰੇਰੀ",
      th: "ไลบรารี",
      "zh-Hant": "資料庫"
    },
    storage: {
      it: "Archiviazione",
      tr: "Depolama",
      vi: "Lưu trữ",
      nl: "Opslag",
      hi: "स्टोरेज",
      bn: "স্টোরেজ",
      ta: "சேமிப்பு",
      te: "నిల్వ",
      mr: "साठवण",
      ur: "اسٹوریج",
      gu: "સંગ્રહ",
      kn: "ಸಂಗ್ರಹಣೆ",
      ml: "സംഭരണം",
      pa: "ਸਟੋਰੇਜ",
      th: "พื้นที่จัดเก็บ",
      "zh-Hant": "儲存空間"
    },
    architecture: {
      it: "Architettura",
      tr: "Mimari",
      vi: "Kiến trúc",
      nl: "Architectuur",
      hi: "आर्किटेक्चर",
      bn: "আর্কিটেকচার",
      ta: "கட்டமைப்பு",
      te: "నిర్మాణం",
      mr: "रचना",
      ur: "ساخت",
      gu: "રચના",
      kn: "ವಾಸ್ತುಶಿಲ್ಪ",
      ml: "ആർക്കിടെക്ചർ",
      pa: "ਆਰਕੀਟੈਕਚਰ",
      th: "สถาปัตยกรรม",
      "zh-Hant": "架構"
    }
  },
  pages: {
    installation: {
      it: "Installazione",
      tr: "Kurulum",
      vi: "Cài đặt",
      nl: "Installatie",
      hi: "इंस्टॉलेशन",
      bn: "ইনস্টলেশন",
      ta: "நிறுவல்",
      te: "ఇన్‌స్టాలేషన్",
      mr: "स्थापना",
      ur: "تنصیب",
      gu: "ઇન્સ્ટોલેશન",
      kn: "ಸ್ಥಾಪನೆ",
      ml: "ഇൻസ്റ്റാളേഷൻ",
      pa: "ਇੰਸਟਾਲੇਸ਼ਨ",
      th: "การติดตั้ง",
      "zh-Hant": "安裝"
    },
    quickStart: {
      it: "Avvio rapido",
      tr: "Hızlı başlangıç",
      vi: "Bắt đầu nhanh",
      nl: "Snelstart",
      hi: "त्वरित शुरुआत",
      bn: "দ্রুত শুরু",
      ta: "விரைவு தொடக்கம்",
      te: "త్వరిత ప్రారంభం",
      mr: "जलद सुरुवात",
      ur: "فوری آغاز",
      gu: "ઝડપી શરૂઆત",
      kn: "ತ್ವರಿತ ಪ್ರಾರಂಭ",
      ml: "ദ്രുതാരംഭം",
      pa: "ਤੁਰੰਤ ਸ਼ੁਰੂਆਤ",
      th: "เริ่มใช้อย่างรวดเร็ว",
      "zh-Hant": "快速開始"
    },
    importing: {
      it: "Importazione",
      tr: "İçe aktarma",
      vi: "Nhập",
      nl: "Importeren",
      hi: "इम्पोर्ट",
      bn: "ইম্পোর্ট",
      ta: "இறக்குமதி",
      te: "దిగుమతి",
      mr: "आयात",
      ur: "درآمد",
      gu: "આયાત",
      kn: "ಆಮದು",
      ml: "ഇറക്കുമതി",
      pa: "ਆਯਾਤ",
      th: "นำเข้า",
      "zh-Hant": "匯入"
    },
    metadata: {
      it: "Metadati",
      tr: "Üst veri",
      vi: "Siêu dữ liệu",
      nl: "Metadata",
      hi: "मेटाडेटा",
      bn: "মেটাডেটা",
      ta: "மெட்டாடேட்டா",
      te: "మెటాడేటా",
      mr: "मेटाडेटा",
      ur: "میٹا ڈیٹا",
      gu: "મેટાડેટા",
      kn: "ಮೆಟಾಡೇಟಾ",
      ml: "മെറ്റാഡാറ്റ",
      pa: "ਮੈਟਾਡਾਟਾ",
      th: "เมทาดาทา",
      "zh-Hant": "中繼資料"
    },
    browsing: {
      it: "Navigazione",
      tr: "Göz atma",
      vi: "Duyệt",
      nl: "Bladeren",
      hi: "ब्राउज़िंग",
      bn: "ব্রাউজ",
      ta: "உலாவல்",
      te: "బ్రౌజింగ్",
      mr: "ब्राउझिंग",
      ur: "براؤزنگ",
      gu: "બ્રાઉઝિંગ",
      kn: "ಬ್ರೌಸಿಂಗ್",
      ml: "ബ്രൗസിംഗ്",
      pa: "ਝਲਕਣਾ",
      th: "เรียกดู",
      "zh-Hant": "瀏覽"
    },
    overview: {
      it: "Panoramica",
      tr: "Genel bakış",
      vi: "Tổng quan",
      nl: "Overzicht",
      hi: "ओवरव्यू",
      bn: "ওভারভিউ",
      ta: "மேலோட்டம்",
      te: "అవలోకనం",
      mr: "आढावा",
      ur: "جائزہ",
      gu: "ઝાંખી",
      kn: "ಅವಲೋಕನ",
      ml: "അവലോകനം",
      pa: "ਝਲਕ",
      th: "ภาพรวม",
      "zh-Hant": "概覽"
    },
    sync: {
      it: "Sincronizzazione",
      tr: "Eşzamanlama",
      vi: "Đồng bộ",
      nl: "Synchronisatie",
      hi: "सिंक",
      bn: "সিঙ্ক",
      ta: "ஒத்திசைவு",
      te: "సింక్",
      mr: "समक्रमण",
      ur: "مطابقت پذیری",
      gu: "સિંક",
      kn: "ಸಿಂಕ್",
      ml: "സിങ്ക്",
      pa: "ਸਿੰਕ",
      th: "ซิงค์",
      "zh-Hant": "同步"
    },
    dataModel: {
      it: "Modello dati",
      tr: "Veri modeli",
      vi: "Mô hình dữ liệu",
      nl: "Datamodel",
      hi: "डेटा मॉडल",
      bn: "ডেটা মডেল",
      ta: "தரவு மாதிரி",
      te: "డేటా మోడల్",
      mr: "डेटा मॉडेल",
      ur: "ڈیٹا ماڈل",
      gu: "ડેટા મોડેલ",
      kn: "ಡೇಟಾ ಮಾದರಿ",
      ml: "ഡാറ്റ മോഡൽ",
      pa: "ਡਾਟਾ ਮਾਡਲ",
      th: "โมเดลข้อมูล",
      "zh-Hant": "資料模型"
    },
    cloudHome: {
      it: "Cloud home",
      tr: "Bulut evi",
      vi: "Cloud home",
      nl: "Cloud home",
      hi: "क्लाउड होम",
      bn: "ক্লাউড হোম",
      ta: "கிளவுட் ஹோம்",
      te: "క్లౌడ్ హోమ్",
      mr: "क्लाउड होम",
      ur: "کلاؤڈ ہوم",
      gu: "ક્લાઉડ હોમ",
      kn: "ಕ್ಲೌಡ್ ಹೋಮ್",
      ml: "ക്ലൗഡ് ഹോം",
      pa: "ਕਲਾਉਡ ਹੋਮ",
      th: "Cloud home",
      "zh-Hant": "雲端位置"
    },
    encryption: {
      it: "Crittografia",
      tr: "Şifreleme",
      vi: "Mã hóa",
      nl: "Versleuteling",
      hi: "एन्क्रिप्शन",
      bn: "এনক্রিপশন",
      ta: "குறியாக்கம்",
      te: "ఎన్‌క్రిప్షన్",
      mr: "कूटबद्धीकरण",
      ur: "خفیہ کاری",
      gu: "એન્ક્રિપ્શન",
      kn: "ಎನ್‌ಕ್ರಿಪ್ಷನ್",
      ml: "എൻക്രിപ്ഷൻ",
      pa: "ਇਨਕ੍ਰਿਪਸ਼ਨ",
      th: "การเข้ารหัส",
      "zh-Hant": "加密"
    },
    membership: {
      it: "Appartenenza",
      tr: "Üyelik",
      vi: "Thành viên",
      nl: "Lidmaatschap",
      hi: "सदस्यता",
      bn: "সদস্যতা",
      ta: "உறுப்பினர் நிலை",
      te: "సభ్యత్వం",
      mr: "सदस्यत्व",
      ur: "رکنیت",
      gu: "સભ્યતા",
      kn: "ಸದಸ್ಯತ್ವ",
      ml: "അംഗത്വം",
      pa: "ਮੈਂਬਰਸ਼ਿਪ",
      th: "สมาชิกภาพ",
      "zh-Hant": "成員資格"
    },
    serverless: {
      it: "Senza server",
      tr: "Sunucusuz",
      vi: "Không máy chủ",
      nl: "Serverloos",
      hi: "सर्वर रहित",
      bn: "সার্ভারবিহীন",
      ta: "சர்வர் இல்லாமல்",
      te: "సర్వర్‌లెస్",
      mr: "सर्व्हरविरहित",
      ur: "بغیر سرور",
      gu: "સર્વર વિના",
      kn: "ಸರ್ವರ್ ಇಲ್ಲದೆ",
      ml: "സർവർ ഇല്ലാതെ",
      pa: "ਸਰਵਰ ਰਹਿਤ",
      th: "ไร้เซิร์ฟเวอร์",
      "zh-Hant": "無伺服器"
    }
  }
};
const addedLanding = {
  "zh-Hant": {
    metaDescription: "bae 是一個音樂資料庫，用來管理你擁有的專輯，可在手機與桌面端使用，並透過你已有的雲端儲存同步。bae 仍在 1.0 之前，不適合一般使用。",
    title: "bae：你的音樂，隨處可用",
    navDocs: "文件",
    navDownload: "下載",
    languageLabel: "語言",
    heroTitle: [
      "你的音樂",
      "隨處可用"
    ],
    heroText: "你的全部音樂，都在手機與桌面端。透過你的雲端儲存在裝置間同步，不需要執行伺服器。",
    heroBold: "私有，屬於你。",
    statusLabel: "bae 開發狀態",
    statusKicker: "Pre-1.0",
    statusTitle: "不適合一般使用。",
    statusText: "僅限測試建置。資料與同步格式會在沒有遷移的情況下變更。",
    downloadMac: "下載 macOS 版",
    seeFeatures: "查看功能",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "屬於你",
        "你的檔案仍然屬於你"
      ],
      [
        "私有",
        "你的裝置保存金鑰"
      ],
      [
        "隨處可用",
        "透過你的雲同步"
      ]
    ],
    sheepAlt: "戴著耳機的 bae 羊",
    noteSynced: "已同步",
    noteSyncedSub: "跨你的裝置",
    noteCloud: "你的雲",
    noteCloudSub: "無伺服器",
    syncEyebrow: "加密同步",
    syncTitle: [
      "只屬於你，位於",
      "每台裝置"
    ],
    syncText: "你的裝置透過你選擇的雲端儲存同步。無需執行伺服器。所有內容離開裝置前都會加密。只有你能讀取。",
    desktop: "你的桌面端",
    phone: "你的手機",
    holdsKeys: "保存金鑰",
    encrypted: "已加密",
    cloudStorage: "你的雲端儲存",
    encryptionLink: "查看加密如何運作 →",
    libraryEyebrow: "資料庫",
    libraryTitle: [
      "你的整個收藏，",
      "按發行版本準確整理"
    ],
    libraryText: "把你擁有的專輯放進一個資料庫，然後帶到任何地方。",
    cards: [
      [
        "發行版本，而不是資料夾",
        "把音樂資料夾交給 bae，bae 會協助把每個資料夾匹配到正確的發行版本。同一張專輯的不同發行版本會分組顯示。",
        [
          "中繼資料來源",
          "封面圖",
          "發行版本分組"
        ]
      ],
      [
        "整個資料庫，每台裝置",
        "你的雲保存資料庫。每台裝置都能看到全部內容。曲目播放時下載；固定的發行版本可離線播放。",
        [
          "手機與桌面端",
          "離線固定"
        ]
      ],
      [
        "播放細節",
        "專輯保留播放細節：黑膠與卡帶錄音會在換面時暫停。CUE 表可用於播放 CD pregap。",
        [
          "CD pregap",
          "黑膠換面"
        ]
      ],
      [
        "由你控制",
        "你的音樂仍然屬於你。你的檔案留在你放置的位置。你控制自己的資料庫。",
        [
          "你的檔案",
          "無鎖定"
        ]
      ]
    ],
    engineEyebrow: "內部結構",
    engineTitle: [
      "每個平台上的",
      "原生應用"
    ],
    engineText: "每個應用都是對應平台的原生應用。它們都使用同一個 Rust 核心來處理資料庫、播放、同步和加密。",
    rustCore: "Rust 核心",
    rustCoreSub: "資料庫 · 播放 · 同步 · 加密",
    sharedRustCore: "共享 Rust 核心",
    deps: [
      [
        "FFmpeg",
        "音訊解碼"
      ],
      [
        "SQLite",
        "資料庫"
      ],
      [
        "雲端儲存",
        "同步後端"
      ]
    ],
    playbackEyebrow: "播放",
    playbackTitle: "圍繞你的檔案建構",
    minis: [
      [
        "原始品質",
        "你的檔案會保持原樣。播放使用原始檔案。"
      ],
      [
        "本機優先",
        "在不更改檔案的情況下把資料夾匹配到發行版本。需要同步時再加入雲端儲存。"
      ],
      [
        "離線播放",
        "把發行版本固定到裝置。它們會繼續離線播放。"
      ]
    ],
    endAria: "把你的收藏帶回家",
    endPrefix: "把你的收藏帶到",
    endFallback: "身邊。",
    endText: "私有、開源，並處於開發中。你加入的一切仍然屬於你。",
    readArchitecture: "閱讀架構",
    footerStatus: "私有且開源",
    cycleWords: [
      "身邊",
      "任何地方",
      "一起",
      "離線",
      "由你控制"
    ]
  },
  it: {
    metaDescription: "bae: la tua musica ovunque. Tutta la tua musica sul telefono e sul desktop. Ascolta su più dispositivi, sincronizzati tramite il tuo cloud. Nessun server da mantenere.",
    title: "bae: la tua musica ovunque",
    navDocs: "Documenti",
    navDownload: "Scarica",
    languageLabel: "Lingua",
    heroTitle: [
      "La tua musica",
      "ovunque"
    ],
    heroText: "Tutta la tua musica sul telefono e sul desktop. Ascolta su più dispositivi, sincronizzati tramite il tuo cloud. Nessun server da mantenere.",
    heroBold: "Privata, tua.",
    statusLabel: "stato di sviluppo di bae",
    statusKicker: "Pre-1.0",
    statusTitle: "Non pronto per l’uso generale.",
    statusText: "Solo build di prova. I formati di dati e sincronizzazione cambieranno senza migrazione.",
    downloadMac: "Scarica per macOS",
    seeFeatures: "Vedi funzioni",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "Tua",
        "i file restano tuoi"
      ],
      [
        "Privata",
        "i tuoi dispositivi conservano le chiavi"
      ],
      [
        "Ovunque",
        "sincronizzata tramite il tuo cloud"
      ]
    ],
    sheepAlt: "la pecora bae con le cuffie",
    noteSynced: "Sincronizzazione",
    noteSyncedSub: "tra i tuoi dispositivi",
    noteCloud: "Il tuo cloud",
    noteCloudSub: "nessun server",
    syncEyebrow: "Sincronizzazione cifrata",
    syncTitle: [
      "La tua musica",
      "ovunque"
    ],
    syncText: "La tua libreria si sincronizza tramite il cloud che scegli. Tutto viene cifrato prima di lasciare il dispositivo. Solo tu puoi leggerlo.",
    desktop: "Desktop",
    phone: "Telefono",
    holdsKeys: "conserva le chiavi",
    encrypted: "Crittografia",
    cloudStorage: "Archiviazione",
    encryptionLink: "Crittografia →",
    libraryEyebrow: "Libreria",
    libraryTitle: [
      "Tutta la tua collezione,",
      "precisa per edizione"
    ],
    libraryText: "Porta gli album che possiedi in una sola libreria e portali con te.",
    cards: [
      [
        "Release, non cartelle",
        "bae abbina le cartelle musicali ai metadati della release e raggruppa release diverse dello stesso album.",
        [
          "Metadati",
          "Copertina",
          "Release"
        ]
      ],
      [
        "Tutta la libreria, ogni dispositivo",
        "Il tuo cloud conserva la libreria. Le tracce vengono scaricate durante la riproduzione; le release fissate funzionano offline.",
        [
          "Telefono e desktop",
          "Offline"
        ]
      ],
      [
        "Dettagli di riproduzione",
        "Gli album conservano dettagli come cambi lato e pregap CUE.",
        [
          "Pregap CD",
          "Cambi lato"
        ]
      ],
      [
        "Sotto il tuo controllo",
        "La musica e i file restano tuoi.",
        [
          "I tuoi file",
          "Nessun lock-in"
        ]
      ]
    ],
    engineEyebrow: "Sotto il cofano",
    engineTitle: [
      "App native",
      "su ogni piattaforma"
    ],
    engineText: "Ogni app è nativa della sua piattaforma e usa lo stesso core Rust per libreria, riproduzione, sincronizzazione e cifratura.",
    rustCore: "Core Rust",
    rustCoreSub: "libreria · riproduzione · sincronizzazione · cifratura",
    sharedRustCore: "core Rust condiviso",
    deps: [
      [
        "FFmpeg",
        "decodifica audio"
      ],
      [
        "SQLite",
        "database libreria"
      ],
      [
        "Storage cloud",
        "sync"
      ]
    ],
    playbackEyebrow: "Riproduzione",
    playbackTitle: "Costruito sui tuoi file",
    minis: [
      [
        "Qualità originale",
        "La riproduzione usa i file originali."
      ],
      [
        "Prima locale",
        "Abbina cartelle senza modificare i file."
      ],
      [
        "Offline",
        "Fissa le release per riprodurle offline."
      ]
    ],
    endAria: "Leggi architettura",
    endPrefix: "Leggi architettura",
    endFallback: ".",
    endText: "Privato, open source e in sviluppo. Tutto ciò che aggiungi resta tuo.",
    readArchitecture: "Leggi architettura",
    footerStatus: "privato e open source",
    cycleWords: [
      "a casa",
      "ovunque",
      "insieme",
      "offline",
      "sotto controllo"
    ]
  },
  tr: {
    metaDescription: "bae: müziğin her yerde. Tüm müziğin telefonda ve masaüstünde. Kendi bulut depolaman üzerinden cihazlar arasında dinle. Çalıştırman gereken sunucu yok.",
    title: "bae: müziğin her yerde",
    navDocs: "Belgeler",
    navDownload: "İndir",
    languageLabel: "Dil",
    heroTitle: [
      "Müziğin",
      "her yerde"
    ],
    heroText: "Tüm müziğin telefonda ve masaüstünde. Kendi bulut depolaman üzerinden cihazlar arasında dinle. Çalıştırman gereken sunucu yok.",
    heroBold: "Özel, senin.",
    statusLabel: "bae geliştirme durumu",
    statusKicker: "Pre-1.0",
    statusTitle: "Genel kullanıma hazır değil.",
    statusText: "Yalnızca test derlemeleri. Veri ve eşzamanlama biçimleri geçiş olmadan değişecek.",
    downloadMac: "macOS için indir",
    seeFeatures: "Özellikleri gör",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "Senin",
        "dosyaların sende kalır"
      ],
      [
        "Özel",
        "anahtarlar cihazlarında durur"
      ],
      [
        "Her yerde",
        "kendi bulutunla eşzamanlanır"
      ]
    ],
    sheepAlt: "kulaklık takan bae koyunu",
    noteSynced: "Eşzamanlama",
    noteSyncedSub: "cihazların arasında",
    noteCloud: "Bulutun",
    noteCloudSub: "sunucu yok",
    syncEyebrow: "Şifreli eşzamanlama",
    syncTitle: [
      "Müziğin",
      "her yerde"
    ],
    syncText: "Cihazların seçtiğin bulut depolama üzerinden eşzamanlanır. Her şey cihazından çıkmadan önce şifrelenir. Sadece sen okuyabilirsin.",
    desktop: "Masaüstü",
    phone: "Telefon",
    holdsKeys: "anahtarları tutar",
    encrypted: "Şifreleme",
    cloudStorage: "Depolama",
    encryptionLink: "Şifreleme →",
    libraryEyebrow: "Kitaplık",
    libraryTitle: [
      "Tüm koleksiyonun,",
      "sürüme göre doğru"
    ],
    libraryText: "Sahip olduğun albümleri tek bir kitaplığa koy ve her yere taşı.",
    cards: [
      [
        "Klasör değil, sürüm",
        "bae müzik klasörlerini sürüm metadatasıyla eşleştirir ve aynı albümün farklı sürümlerini birlikte gösterir.",
        [
          "Metadata",
          "Kapak",
          "Sürümler"
        ]
      ],
      [
        "Tüm kitaplık, her cihaz",
        "Bulutun kitaplığı tutar. Parçalar çalarken indirilir; sabitlenen sürümler çevrimdışı çalar.",
        [
          "Telefon ve masaüstü",
          "Çevrimdışı"
        ]
      ],
      [
        "Çalma ayrıntıları",
        "Albümler yüz geçişleri ve CUE pregap gibi çalma ayrıntılarını korur.",
        [
          "CD pregap",
          "Yüz geçişleri"
        ]
      ],
      [
        "Kontrol sende",
        "Müziğin ve dosyaların senindir.",
        [
          "Dosyaların",
          "Kilitlenme yok"
        ]
      ]
    ],
    engineEyebrow: "İç yapı",
    engineTitle: [
      "Her platformda",
      "yerel uygulama"
    ],
    engineText: "Her uygulama kendi platformuna özgüdür ve kitaplık, çalma, eşzamanlama ve şifreleme için aynı Rust çekirdeğini kullanır.",
    rustCore: "Rust çekirdeği",
    rustCoreSub: "kitaplık · çalma · eşzamanlama · şifreleme",
    sharedRustCore: "paylaşılan Rust çekirdeği",
    deps: [
      [
        "FFmpeg",
        "ses çözme"
      ],
      [
        "SQLite",
        "kitaplık veritabanı"
      ],
      [
        "Bulut depolama",
        "eşzamanlama"
      ]
    ],
    playbackEyebrow: "Çalma",
    playbackTitle: "Dosyalarının etrafında kurulu",
    minis: [
      [
        "Özgün kalite",
        "Çalma özgün dosyalarını kullanır."
      ],
      [
        "Önce yerel",
        "Dosyaları değiştirmeden klasörleri eşleştir."
      ],
      [
        "Çevrimdışı",
        "Çevrimdışı çalmak için sürümleri sabitle."
      ]
    ],
    endAria: "Mimariyi oku",
    endPrefix: "Mimariyi oku",
    endFallback: ".",
    endText: "Özel, açık kaynak ve geliştirme aşamasında. Eklediğin her şey senin kalır.",
    readArchitecture: "Mimariyi oku",
    footerStatus: "özel ve açık kaynak",
    cycleWords: [
      "eve",
      "her yere",
      "birlikte",
      "çevrimdışı",
      "kontrol altında"
    ]
  },
  vi: {
    metaDescription: "bae: nhạc của bạn ở mọi nơi. Toàn bộ nhạc của bạn trên điện thoại và máy tính. Nghe trên nhiều thiết bị, đồng bộ qua bộ nhớ đám mây của bạn. Không có máy chủ phải vận hành.",
    title: "bae: nhạc của bạn ở mọi nơi",
    navDocs: "Tài liệu",
    navDownload: "Tải xuống",
    languageLabel: "Ngôn ngữ",
    heroTitle: [
      "Nhạc của bạn",
      "ở mọi nơi"
    ],
    heroText: "Toàn bộ nhạc của bạn trên điện thoại và máy tính. Nghe trên nhiều thiết bị, đồng bộ qua bộ nhớ đám mây của bạn. Không có máy chủ phải vận hành.",
    heroBold: "Riêng tư, của bạn.",
    statusLabel: "trạng thái phát triển bae",
    statusKicker: "Pre-1.0",
    statusTitle: "Chưa sẵn sàng cho sử dụng chung.",
    statusText: "Chỉ dành cho bản thử nghiệm. Dạng dữ liệu và đồng bộ sẽ thay đổi không kèm chuyển đổi.",
    downloadMac: "Tải cho macOS",
    seeFeatures: "Xem tính năng",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "Của bạn",
        "file vẫn là của bạn"
      ],
      [
        "Riêng tư",
        "thiết bị của bạn giữ khóa"
      ],
      [
        "Mọi nơi",
        "đồng bộ qua cloud của bạn"
      ]
    ],
    sheepAlt: "cừu bae đeo tai nghe",
    noteSynced: "Đồng bộ",
    noteSyncedSub: "giữa các thiết bị",
    noteCloud: "Cloud của bạn",
    noteCloudSub: "không máy chủ",
    syncEyebrow: "Đồng bộ mã hóa",
    syncTitle: [
      "Nhạc của bạn",
      "ở mọi nơi"
    ],
    syncText: "Thiết bị của bạn đồng bộ qua bộ nhớ đám mây bạn chọn. Mọi thứ được mã hóa trước khi rời thiết bị. Chỉ bạn đọc được.",
    desktop: "Máy tính",
    phone: "Điện thoại",
    holdsKeys: "giữ khóa",
    encrypted: "Mã hóa",
    cloudStorage: "Lưu trữ",
    encryptionLink: "Mã hóa →",
    libraryEyebrow: "Thư viện",
    libraryTitle: [
      "Toàn bộ bộ sưu tập,",
      "đúng theo bản phát hành"
    ],
    libraryText: "Đưa các album bạn sở hữu vào một thư viện rồi mang đi mọi nơi.",
    cards: [
      [
        "Bản phát hành, không phải thư mục",
        "bae ghép thư mục nhạc với metadata bản phát hành và nhóm các bản khác nhau của cùng album.",
        [
          "Metadata",
          "Bìa",
          "Bản phát hành"
        ]
      ],
      [
        "Cả thư viện, mọi thiết bị",
        "Cloud của bạn giữ thư viện. Track tải xuống khi phát; bản được ghim phát offline.",
        [
          "Điện thoại và máy tính",
          "Offline"
        ]
      ],
      [
        "Chi tiết phát",
        "Album giữ chi tiết như đổi mặt đĩa và CUE pregap.",
        [
          "CD pregap",
          "Đổi mặt"
        ]
      ],
      [
        "Bạn kiểm soát",
        "Nhạc và file vẫn là của bạn.",
        [
          "File của bạn",
          "Không khóa"
        ]
      ]
    ],
    engineEyebrow: "Bên trong",
    engineTitle: [
      "Ứng dụng native",
      "trên mọi nền tảng"
    ],
    engineText: "Mỗi ứng dụng là native cho nền tảng của nó và dùng cùng Rust core cho thư viện, phát, đồng bộ và mã hóa.",
    rustCore: "Rust core",
    rustCoreSub: "thư viện · phát · đồng bộ · mã hóa",
    sharedRustCore: "Rust core chung",
    deps: [
      [
        "FFmpeg",
        "giải mã âm thanh"
      ],
      [
        "SQLite",
        "cơ sở dữ liệu thư viện"
      ],
      [
        "Lưu trữ cloud",
        "đồng bộ"
      ]
    ],
    playbackEyebrow: "Phát nhạc",
    playbackTitle: "Xây quanh file của bạn",
    minis: [
      [
        "Chất lượng gốc",
        "Phát bằng file gốc của bạn."
      ],
      [
        "Cục bộ trước",
        "Ghép thư mục mà không đổi file."
      ],
      [
        "Offline",
        "Ghim bản phát hành để nghe offline."
      ]
    ],
    endAria: "Đọc kiến trúc",
    endPrefix: "Đọc kiến trúc",
    endFallback: ".",
    endText: "Riêng tư, mã nguồn mở và đang phát triển. Mọi thứ bạn thêm vẫn là của bạn.",
    readArchitecture: "Đọc kiến trúc",
    footerStatus: "riêng tư và mã nguồn mở",
    cycleWords: [
      "về nhà",
      "mọi nơi",
      "cùng nhau",
      "ngoại tuyến",
      "trong quyền kiểm soát"
    ]
  },
  nl: {
    metaDescription: "bae: je muziek overal. Al je muziek op telefoon en desktop. Luister op al je apparaten, gesynchroniseerd via je eigen cloudopslag. Geen server om te beheren.",
    title: "bae: je muziek overal",
    navDocs: "Docs",
    navDownload: "Download",
    languageLabel: "Taal",
    heroTitle: [
      "Je muziek",
      "overal"
    ],
    heroText: "Al je muziek op telefoon en desktop. Luister op al je apparaten, gesynchroniseerd via je eigen cloudopslag. Geen server om te beheren.",
    heroBold: "Privé, van jou.",
    statusLabel: "ontwikkelstatus van bae",
    statusKicker: "Pre-1.0",
    statusTitle: "Niet klaar voor algemeen gebruik.",
    statusText: "Alleen testbuilds. Data- en synchronisatieformaten veranderen zonder migratie.",
    downloadMac: "Download voor macOS",
    seeFeatures: "Bekijk functies",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "Van jou",
        "je bestanden blijven van jou"
      ],
      [
        "Privé",
        "je apparaten bewaren de sleutels"
      ],
      [
        "Overal",
        "gesynchroniseerd via je cloud"
      ]
    ],
    sheepAlt: "bae het schaap met koptelefoon",
    noteSynced: "Synchronisatie",
    noteSyncedSub: "tussen je apparaten",
    noteCloud: "Je cloud",
    noteCloudSub: "geen server",
    syncEyebrow: "Versleutelde synchronisatie",
    syncTitle: [
      "Je muziek",
      "overal"
    ],
    syncText: "Je apparaten synchroniseren via de cloudopslag die jij kiest. Alles wordt versleuteld voordat het je apparaat verlaat. Alleen jij kunt het lezen.",
    desktop: "Desktop",
    phone: "Telefoon",
    holdsKeys: "bewaart de sleutels",
    encrypted: "Versleuteling",
    cloudStorage: "Opslag",
    encryptionLink: "Versleuteling →",
    libraryEyebrow: "Bibliotheek",
    libraryTitle: [
      "Je hele collectie,",
      "kloppend per uitgave"
    ],
    libraryText: "Zet de albums die je bezit in één bibliotheek en neem ze overal mee.",
    cards: [
      [
        "Uitgaven, geen mappen",
        "bae koppelt je muziekmappen aan uitgavemetadata en groepeert verschillende uitgaven van hetzelfde album.",
        [
          "Metadata",
          "Hoes",
          "Uitgaven"
        ]
      ],
      [
        "Hele bibliotheek, elk apparaat",
        "Je cloud bewaart de bibliotheek. Tracks downloaden tijdens het afspelen; vastgezette uitgaven spelen offline.",
        [
          "Telefoon en desktop",
          "Offline"
        ]
      ],
      [
        "Afspeeldetails",
        "Albums bewaren details zoals kantwissels en CUE-pregaps.",
        [
          "CD-pregaps",
          "Kantwissels"
        ]
      ],
      [
        "Jij beheert het",
        "Je muziek en bestanden blijven van jou.",
        [
          "Je bestanden",
          "Geen lock-in"
        ]
      ]
    ],
    engineEyebrow: "Onder de motorkap",
    engineTitle: [
      "Native apps",
      "op elk platform"
    ],
    engineText: "Elke app is native voor het platform en gebruikt dezelfde Rust-kern voor bibliotheek, afspelen, synchronisatie en versleuteling.",
    rustCore: "Rust-kern",
    rustCoreSub: "bibliotheek · afspelen · synchronisatie · versleuteling",
    sharedRustCore: "gedeelde Rust-kern",
    deps: [
      [
        "FFmpeg",
        "audio decoderen"
      ],
      [
        "SQLite",
        "bibliotheekdatabase"
      ],
      [
        "Cloudopslag",
        "synchronisatie"
      ]
    ],
    playbackEyebrow: "Afspelen",
    playbackTitle: "Gebouwd rond je bestanden",
    minis: [
      [
        "Originele kwaliteit",
        "Afspelen gebruikt je originele bestanden."
      ],
      [
        "Lokaal eerst",
        "Koppel mappen zonder bestanden te wijzigen."
      ],
      [
        "Offline",
        "Zet uitgaven vast om offline te spelen."
      ]
    ],
    endAria: "Lees architectuur",
    endPrefix: "Lees architectuur",
    endFallback: ".",
    endText: "Privé, open source en in ontwikkeling. Alles wat je toevoegt blijft van jou.",
    readArchitecture: "Lees architectuur",
    footerStatus: "privé en open source",
    cycleWords: [
      "naar huis",
      "overal",
      "samen",
      "offline",
      "onder controle"
    ]
  },
  hi: {
    metaDescription: "bae: आपका संगीत हर जगह. आपका पूरा संगीत फ़ोन और डेस्कटॉप पर। अपने क्लाउड स्टोरेज से डिवाइसों के बीच सुनें। चलाने के लिए कोई सर्वर नहीं।",
    title: "bae: आपका संगीत हर जगह",
    navDocs: "दस्तावेज़",
    navDownload: "डाउनलोड",
    languageLabel: "भाषा",
    heroTitle: [
      "आपका संगीत",
      "हर जगह"
    ],
    heroText: "आपका पूरा संगीत फ़ोन और डेस्कटॉप पर। अपने क्लाउड स्टोरेज से डिवाइसों के बीच सुनें। चलाने के लिए कोई सर्वर नहीं।",
    heroBold: "निजी, आपका।",
    statusLabel: "bae विकास स्थिति",
    statusKicker: "Pre-1.0",
    statusTitle: "सामान्य उपयोग के लिए तैयार नहीं।",
    statusText: "केवल परीक्षण बिल्ड। डेटा और सिंक फ़ॉर्मेट बिना माइग्रेशन बदलेंगे।",
    downloadMac: "macOS के लिए डाउनलोड",
    seeFeatures: "सुविधाएँ देखें",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "आपका संगीत",
        "आपकी फ़ाइलें आपकी रहती हैं"
      ],
      [
        "निजी",
        "आपके डिवाइस कुंजियाँ रखते हैं"
      ],
      [
        "हर जगह",
        "आपके क्लाउड से सिंक"
      ]
    ],
    sheepAlt: "आपका संगीत",
    noteSynced: "सिंक",
    noteSyncedSub: "डिवाइसों के बीच",
    noteCloud: "आपका क्लाउड",
    noteCloudSub: "सर्वर नहीं",
    syncEyebrow: "एन्क्रिप्टेड सिंक",
    syncTitle: [
      "आपका संगीत",
      "हर जगह"
    ],
    syncText: "आपके डिवाइस आपके चुने हुए क्लाउड स्टोरेज से सिंक होते हैं। सब कुछ डिवाइस छोड़ने से पहले एन्क्रिप्ट होता है। केवल आप पढ़ सकते हैं।",
    desktop: "डेस्कटॉप",
    phone: "फ़ोन",
    holdsKeys: "कुंजियाँ रखता है",
    encrypted: "एन्क्रिप्शन",
    cloudStorage: "स्टोरेज",
    encryptionLink: "एन्क्रिप्शन →",
    libraryEyebrow: "लाइब्रेरी",
    libraryTitle: [
      "आपका पूरा संग्रह,",
      "रिलीज़ के हिसाब से सही"
    ],
    libraryText: "अपने एल्बमों को एक लाइब्रेरी में रखें और उन्हें कहीं भी ले जाएँ।",
    cards: [
      [
        "रिलीज़ के हिसाब से सही",
        "अपने एल्बमों को एक लाइब्रेरी में रखें और उन्हें कहीं भी ले जाएँ।",
        [
          "मेटाडेटा",
          "ओवरव्यू",
          "सिंक"
        ]
      ],
      [
        "लाइब्रेरी",
        "आपके डिवाइस आपके चुने हुए क्लाउड स्टोरेज से सिंक होते हैं। सब कुछ डिवाइस छोड़ने से पहले एन्क्रिप्ट होता है। केवल आप पढ़ सकते हैं।",
        [
          "फ़ोन",
          "डेस्कटॉप"
        ]
      ],
      [
        "एन्क्रिप्शन",
        "आपके डिवाइस आपके चुने हुए क्लाउड स्टोरेज से सिंक होते हैं। सब कुछ डिवाइस छोड़ने से पहले एन्क्रिप्ट होता है। केवल आप पढ़ सकते हैं।",
        [
          "एन्क्रिप्शन",
          "सिंक"
        ]
      ],
      [
        "आपका संगीत",
        "आपकी फ़ाइलें आपकी रहती हैं",
        [
          "आपका संगीत",
          "निजी और ओपन सोर्स"
        ]
      ]
    ],
    engineEyebrow: "आर्किटेक्चर",
    engineTitle: [
      "डेस्कटॉप",
      "फ़ोन"
    ],
    engineText: "आपके डिवाइस आपके चुने हुए क्लाउड स्टोरेज से सिंक होते हैं। सब कुछ डिवाइस छोड़ने से पहले एन्क्रिप्ट होता है। केवल आप पढ़ सकते हैं।",
    rustCore: "Rust core",
    rustCoreSub: "लाइब्रेरी · सिंक · एन्क्रिप्शन",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "लाइब्रेरी"
      ],
      [
        "स्टोरेज",
        "सिंक"
      ]
    ],
    playbackEyebrow: "प्लेबैक",
    playbackTitle: "आपका संगीत हर जगह",
    minis: [
      [
        "आपका संगीत",
        "आपकी फ़ाइलें आपकी रहती हैं"
      ],
      [
        "स्टोरेज",
        "आपके डिवाइस आपके चुने हुए क्लाउड स्टोरेज से सिंक होते हैं। सब कुछ डिवाइस छोड़ने से पहले एन्क्रिप्ट होता है। केवल आप पढ़ सकते हैं।"
      ],
      [
        "सिंक",
        "आपके क्लाउड से सिंक"
      ]
    ],
    endAria: "आर्किटेक्चर पढ़ें",
    endPrefix: "आर्किटेक्चर पढ़ें",
    endFallback: ".",
    endText: "निजी, ओपन सोर्स और विकास में। आप जो जोड़ते हैं वह आपका रहता है।",
    readArchitecture: "आर्किटेक्चर पढ़ें",
    footerStatus: "निजी और ओपन सोर्स",
    cycleWords: [
      "घर",
      "हर जगह",
      "साथ",
      "ऑफ़लाइन",
      "नियंत्रण में"
    ]
  },
  bn: {
    metaDescription: "bae: আপনার সঙ্গীত সবখানে. আপনার সব সঙ্গীত ফোন ও ডেস্কটপে। আপনার ক্লাউড স্টোরেজ দিয়ে ডিভাইসগুলোর মধ্যে শুনুন। চালানোর মতো কোনো সার্ভার নেই।",
    title: "bae: আপনার সঙ্গীত সবখানে",
    navDocs: "ডকুমেন্টেশন",
    navDownload: "ডাউনলোড",
    languageLabel: "ভাষা",
    heroTitle: [
      "আপনার সঙ্গীত",
      "সবখানে"
    ],
    heroText: "আপনার সব সঙ্গীত ফোন ও ডেস্কটপে। আপনার ক্লাউড স্টোরেজ দিয়ে ডিভাইসগুলোর মধ্যে শুনুন। চালানোর মতো কোনো সার্ভার নেই।",
    heroBold: "ব্যক্তিগত, আপনার।",
    statusLabel: "bae উন্নয়নের অবস্থা",
    statusKicker: "Pre-1.0",
    statusTitle: "সাধারণ ব্যবহারের জন্য প্রস্তুত নয়।",
    statusText: "শুধু পরীক্ষামূলক বিল্ড। ডেটা ও সিঙ্ক ফরম্যাট মাইগ্রেশন ছাড়া বদলাবে।",
    downloadMac: "macOS-এর জন্য ডাউনলোড",
    seeFeatures: "বৈশিষ্ট্য দেখুন",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "আপনার সঙ্গীত",
        "আপনার ফাইল আপনারই থাকে"
      ],
      [
        "ব্যক্তিগত",
        "আপনার ডিভাইস কী রাখে"
      ],
      [
        "সবখানে",
        "আপনার ক্লাউড দিয়ে সিঙ্ক"
      ]
    ],
    sheepAlt: "আপনার সঙ্গীত",
    noteSynced: "সিঙ্ক",
    noteSyncedSub: "আপনার ডিভাইসগুলোর মধ্যে",
    noteCloud: "আপনার ক্লাউড",
    noteCloudSub: "সার্ভার নেই",
    syncEyebrow: "এনক্রিপ্টেড সিঙ্ক",
    syncTitle: [
      "আপনার সঙ্গীত",
      "সবখানে"
    ],
    syncText: "আপনার ডিভাইস আপনার বেছে নেওয়া ক্লাউড স্টোরেজ দিয়ে সিঙ্ক হয়। সবকিছু ডিভাইস ছাড়ার আগে এনক্রিপ্ট হয়। শুধু আপনি পড়তে পারেন।",
    desktop: "ডেস্কটপ",
    phone: "ফোন",
    holdsKeys: "কী রাখে",
    encrypted: "এনক্রিপশন",
    cloudStorage: "স্টোরেজ",
    encryptionLink: "এনক্রিপশন →",
    libraryEyebrow: "লাইব্রেরি",
    libraryTitle: [
      "আপনার পুরো সংগ্রহ,",
      "রিলিজ অনুযায়ী সঠিক"
    ],
    libraryText: "আপনার অ্যালবামগুলো এক লাইব্রেরিতে রাখুন এবং যেকোনো জায়গায় নিয়ে যান।",
    cards: [
      [
        "রিলিজ অনুযায়ী সঠিক",
        "আপনার অ্যালবামগুলো এক লাইব্রেরিতে রাখুন এবং যেকোনো জায়গায় নিয়ে যান।",
        [
          "মেটাডেটা",
          "ওভারভিউ",
          "সিঙ্ক"
        ]
      ],
      [
        "লাইব্রেরি",
        "আপনার ডিভাইস আপনার বেছে নেওয়া ক্লাউড স্টোরেজ দিয়ে সিঙ্ক হয়। সবকিছু ডিভাইস ছাড়ার আগে এনক্রিপ্ট হয়। শুধু আপনি পড়তে পারেন।",
        [
          "ফোন",
          "ডেস্কটপ"
        ]
      ],
      [
        "এনক্রিপশন",
        "আপনার ডিভাইস আপনার বেছে নেওয়া ক্লাউড স্টোরেজ দিয়ে সিঙ্ক হয়। সবকিছু ডিভাইস ছাড়ার আগে এনক্রিপ্ট হয়। শুধু আপনি পড়তে পারেন।",
        [
          "এনক্রিপশন",
          "সিঙ্ক"
        ]
      ],
      [
        "আপনার সঙ্গীত",
        "আপনার ফাইল আপনারই থাকে",
        [
          "আপনার সঙ্গীত",
          "ব্যক্তিগত ও ওপেন সোর্স"
        ]
      ]
    ],
    engineEyebrow: "আর্কিটেকচার",
    engineTitle: [
      "ডেস্কটপ",
      "ফোন"
    ],
    engineText: "আপনার ডিভাইস আপনার বেছে নেওয়া ক্লাউড স্টোরেজ দিয়ে সিঙ্ক হয়। সবকিছু ডিভাইস ছাড়ার আগে এনক্রিপ্ট হয়। শুধু আপনি পড়তে পারেন।",
    rustCore: "Rust core",
    rustCoreSub: "লাইব্রেরি · সিঙ্ক · এনক্রিপশন",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "লাইব্রেরি"
      ],
      [
        "স্টোরেজ",
        "সিঙ্ক"
      ]
    ],
    playbackEyebrow: "প্লেব্যাক",
    playbackTitle: "আপনার সঙ্গীত সবখানে",
    minis: [
      [
        "আপনার সঙ্গীত",
        "আপনার ফাইল আপনারই থাকে"
      ],
      [
        "স্টোরেজ",
        "আপনার ডিভাইস আপনার বেছে নেওয়া ক্লাউড স্টোরেজ দিয়ে সিঙ্ক হয়। সবকিছু ডিভাইস ছাড়ার আগে এনক্রিপ্ট হয়। শুধু আপনি পড়তে পারেন।"
      ],
      [
        "সিঙ্ক",
        "আপনার ক্লাউড দিয়ে সিঙ্ক"
      ]
    ],
    endAria: "আর্কিটেকচার পড়ুন",
    endPrefix: "আর্কিটেকচার পড়ুন",
    endFallback: ".",
    endText: "ব্যক্তিগত, ওপেন সোর্স এবং উন্নয়নে। আপনি যা যোগ করেন তা আপনারই থাকে।",
    readArchitecture: "আর্কিটেকচার পড়ুন",
    footerStatus: "ব্যক্তিগত ও ওপেন সোর্স",
    cycleWords: [
      "ঘরে",
      "সবখানে",
      "একসাথে",
      "অফলাইন",
      "নিয়ন্ত্রণে"
    ]
  },
  ta: {
    metaDescription: "bae: உங்கள் இசை எங்கும். உங்கள் முழு இசையும் தொலைபேசி மற்றும் டெஸ்க்டாப்பில். உங்கள் கிளவுட் சேமிப்பகத்தின் மூலம் சாதனங்களுக்கு இடையில் கேளுங்கள். இயக்க வேண்டிய சர்வர் இல்லை.",
    title: "bae: உங்கள் இசை எங்கும்",
    navDocs: "ஆவணங்கள்",
    navDownload: "பதிவிறக்கு",
    languageLabel: "மொழி",
    heroTitle: [
      "உங்கள் இசை",
      "எங்கும்"
    ],
    heroText: "உங்கள் முழு இசையும் தொலைபேசி மற்றும் டெஸ்க்டாப்பில். உங்கள் கிளவுட் சேமிப்பகத்தின் மூலம் சாதனங்களுக்கு இடையில் கேளுங்கள். இயக்க வேண்டிய சர்வர் இல்லை.",
    heroBold: "தனிப்பட்டது, உங்களுடையது.",
    statusLabel: "bae மேம்பாட்டு நிலை",
    statusKicker: "Pre-1.0",
    statusTitle: "பொது பயன்பாட்டுக்கு தயாரில்லை.",
    statusText: "சோதனை build-கள் மட்டுமே. தரவு மற்றும் sync வடிவங்கள் migration இல்லாமல் மாறும்.",
    downloadMac: "macOS-க்கு பதிவிறக்கு",
    seeFeatures: "அம்சங்களைப் பாருங்கள்",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "உங்கள் இசை",
        "உங்கள் கோப்புகள் உங்களிடமே இருக்கும்"
      ],
      [
        "தனிப்பட்டது",
        "உங்கள் சாதனங்கள் விசைகளை வைத்திருக்கும்"
      ],
      [
        "எங்கும்",
        "உங்கள் cloud மூலம் sync"
      ]
    ],
    sheepAlt: "உங்கள் இசை",
    noteSynced: "ஒத்திசைவு",
    noteSyncedSub: "உங்கள் சாதனங்களுக்கு இடையில்",
    noteCloud: "உங்கள் cloud",
    noteCloudSub: "server இல்லை",
    syncEyebrow: "குறியாக்கப்பட்ட sync",
    syncTitle: [
      "உங்கள் இசை",
      "எங்கும்"
    ],
    syncText: "உங்கள் சாதனங்கள் நீங்கள் தேர்ந்தெடுத்த கிளவுட் சேமிப்பகத்தின் மூலம் ஒத்திசைகின்றன. எல்லாம் சாதனத்தை விட்டு செல்லும் முன் குறியாக்கப்படுகிறது. நீங்கள் மட்டுமே படிக்க முடியும்.",
    desktop: "டெஸ்க்டாப்",
    phone: "தொலைபேசி",
    holdsKeys: "விசைகளை வைத்திருக்கும்",
    encrypted: "குறியாக்கம்",
    cloudStorage: "சேமிப்பு",
    encryptionLink: "குறியாக்கம் →",
    libraryEyebrow: "நூலகம்",
    libraryTitle: [
      "உங்கள் முழு சேகரிப்பு,",
      "வெளியீட்டுக்கு துல்லியமாக"
    ],
    libraryText: "உங்களுடைய ஆல்பங்களை ஒரே நூலகத்தில் கொண்டு வந்து எங்கும் எடுத்துச் செல்லுங்கள்.",
    cards: [
      [
        "வெளியீட்டுக்கு துல்லியமாக",
        "உங்களுடைய ஆல்பங்களை ஒரே நூலகத்தில் கொண்டு வந்து எங்கும் எடுத்துச் செல்லுங்கள்.",
        [
          "மெட்டாடேட்டா",
          "மேலோட்டம்",
          "ஒத்திசைவு"
        ]
      ],
      [
        "நூலகம்",
        "உங்கள் சாதனங்கள் நீங்கள் தேர்ந்தெடுத்த கிளவுட் சேமிப்பகத்தின் மூலம் ஒத்திசைகின்றன. எல்லாம் சாதனத்தை விட்டு செல்லும் முன் குறியாக்கப்படுகிறது. நீங்கள் மட்டுமே படிக்க முடியும்.",
        [
          "தொலைபேசி",
          "டெஸ்க்டாப்"
        ]
      ],
      [
        "குறியாக்கம்",
        "உங்கள் சாதனங்கள் நீங்கள் தேர்ந்தெடுத்த கிளவுட் சேமிப்பகத்தின் மூலம் ஒத்திசைகின்றன. எல்லாம் சாதனத்தை விட்டு செல்லும் முன் குறியாக்கப்படுகிறது. நீங்கள் மட்டுமே படிக்க முடியும்.",
        [
          "குறியாக்கம்",
          "ஒத்திசைவு"
        ]
      ],
      [
        "உங்கள் இசை",
        "உங்கள் கோப்புகள் உங்களிடமே இருக்கும்",
        [
          "உங்கள் இசை",
          "தனிப்பட்டது மற்றும் open source"
        ]
      ]
    ],
    engineEyebrow: "கட்டமைப்பு",
    engineTitle: [
      "டெஸ்க்டாப்",
      "தொலைபேசி"
    ],
    engineText: "உங்கள் சாதனங்கள் நீங்கள் தேர்ந்தெடுத்த கிளவுட் சேமிப்பகத்தின் மூலம் ஒத்திசைகின்றன. எல்லாம் சாதனத்தை விட்டு செல்லும் முன் குறியாக்கப்படுகிறது. நீங்கள் மட்டுமே படிக்க முடியும்.",
    rustCore: "Rust core",
    rustCoreSub: "நூலகம் · ஒத்திசைவு · குறியாக்கம்",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "நூலகம்"
      ],
      [
        "சேமிப்பு",
        "ஒத்திசைவு"
      ]
    ],
    playbackEyebrow: "பிளேபேக்",
    playbackTitle: "உங்கள் இசை எங்கும்",
    minis: [
      [
        "உங்கள் இசை",
        "உங்கள் கோப்புகள் உங்களிடமே இருக்கும்"
      ],
      [
        "சேமிப்பு",
        "உங்கள் சாதனங்கள் நீங்கள் தேர்ந்தெடுத்த கிளவுட் சேமிப்பகத்தின் மூலம் ஒத்திசைகின்றன. எல்லாம் சாதனத்தை விட்டு செல்லும் முன் குறியாக்கப்படுகிறது. நீங்கள் மட்டுமே படிக்க முடியும்."
      ],
      [
        "ஒத்திசைவு",
        "உங்கள் cloud மூலம் sync"
      ]
    ],
    endAria: "கட்டமைப்பைப் படிக்க",
    endPrefix: "கட்டமைப்பைப் படிக்க",
    endFallback: ".",
    endText: "தனிப்பட்டது, open source, வளர்ச்சியில். நீங்கள் சேர்ப்பது அனைத்தும் உங்களுடையதே.",
    readArchitecture: "கட்டமைப்பைப் படிக்க",
    footerStatus: "தனிப்பட்டது மற்றும் open source",
    cycleWords: [
      "வீட்டிற்கு",
      "எங்கும்",
      "ஒன்றாக",
      "ஆஃப்லைன்",
      "கட்டுப்பாட்டில்"
    ]
  },
  te: {
    metaDescription: "bae: మీ సంగీతం ఎక్కడైనా. మీ మొత్తం సంగీతం ఫోన్ మరియు డెస్క్‌టాప్‌లో. మీ క్లౌడ్ నిల్వ ద్వారా పరికరాల మధ్య వినండి. నడపాల్సిన సర్వర్ లేదు.",
    title: "bae: మీ సంగీతం ఎక్కడైనా",
    navDocs: "డాక్స్",
    navDownload: "డౌన్‌లోడ్",
    languageLabel: "భాష",
    heroTitle: [
      "మీ సంగీతం",
      "ఎక్కడైనా"
    ],
    heroText: "మీ మొత్తం సంగీతం ఫోన్ మరియు డెస్క్‌టాప్‌లో. మీ క్లౌడ్ నిల్వ ద్వారా పరికరాల మధ్య వినండి. నడపాల్సిన సర్వర్ లేదు.",
    heroBold: "ప్రైవేట్, మీదే.",
    statusLabel: "bae అభివృద్ధి స్థితి",
    statusKicker: "Pre-1.0",
    statusTitle: "సాధారణ వినియోగానికి సిద్ధంగా లేదు.",
    statusText: "పరీక్ష build-లు మాత్రమే. డేటా మరియు sync ఫార్మాట్‌లు migration లేకుండా మారుతాయి.",
    downloadMac: "macOS కోసం డౌన్‌లోడ్",
    seeFeatures: "లక్షణాలు చూడండి",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "మీ సంగీతం",
        "మీ ఫైల్‌లు మీదే"
      ],
      [
        "ప్రైవేట్",
        "మీ పరికరాలు కీలు ఉంచుతాయి"
      ],
      [
        "ఎక్కడైనా",
        "మీ cloud ద్వారా sync"
      ]
    ],
    sheepAlt: "మీ సంగీతం",
    noteSynced: "సింక్",
    noteSyncedSub: "మీ పరికరాల మధ్య",
    noteCloud: "మీ cloud",
    noteCloudSub: "server లేదు",
    syncEyebrow: "ఎన్‌క్రిప్టెడ్ sync",
    syncTitle: [
      "మీ సంగీతం",
      "ఎక్కడైనా"
    ],
    syncText: "మీ పరికరాలు మీరు ఎంచుకున్న క్లౌడ్ నిల్వ ద్వారా సింక్ అవుతాయి. పరికరం విడిచే ముందు అన్నీ ఎన్‌క్రిప్ట్ అవుతాయి. మీరు మాత్రమే చదవగలరు.",
    desktop: "డెస్క్‌టాప్",
    phone: "ఫోన్",
    holdsKeys: "కీలు ఉంచుతుంది",
    encrypted: "ఎన్‌క్రిప్షన్",
    cloudStorage: "నిల్వ",
    encryptionLink: "ఎన్‌క్రిప్షన్ →",
    libraryEyebrow: "లైబ్రరీ",
    libraryTitle: [
      "మీ మొత్తం సేకరణ,",
      "రిలీజ్‌కు సరిపడా ఖచ్చితం"
    ],
    libraryText: "మీ ఆల్బమ్‌లను ఒక లైబ్రరీలో పెట్టి ఎక్కడికైనా తీసుకెళ్లండి.",
    cards: [
      [
        "రిలీజ్‌కు సరిపడా ఖచ్చితం",
        "మీ ఆల్బమ్‌లను ఒక లైబ్రరీలో పెట్టి ఎక్కడికైనా తీసుకెళ్లండి.",
        [
          "మెటాడేటా",
          "అవలోకనం",
          "సింక్"
        ]
      ],
      [
        "లైబ్రరీ",
        "మీ పరికరాలు మీరు ఎంచుకున్న క్లౌడ్ నిల్వ ద్వారా సింక్ అవుతాయి. పరికరం విడిచే ముందు అన్నీ ఎన్‌క్రిప్ట్ అవుతాయి. మీరు మాత్రమే చదవగలరు.",
        [
          "ఫోన్",
          "డెస్క్‌టాప్"
        ]
      ],
      [
        "ఎన్‌క్రిప్షన్",
        "మీ పరికరాలు మీరు ఎంచుకున్న క్లౌడ్ నిల్వ ద్వారా సింక్ అవుతాయి. పరికరం విడిచే ముందు అన్నీ ఎన్‌క్రిప్ట్ అవుతాయి. మీరు మాత్రమే చదవగలరు.",
        [
          "ఎన్‌క్రిప్షన్",
          "సింక్"
        ]
      ],
      [
        "మీ సంగీతం",
        "మీ ఫైల్‌లు మీదే",
        [
          "మీ సంగీతం",
          "ప్రైవేట్ మరియు open source"
        ]
      ]
    ],
    engineEyebrow: "నిర్మాణం",
    engineTitle: [
      "డెస్క్‌టాప్",
      "ఫోన్"
    ],
    engineText: "మీ పరికరాలు మీరు ఎంచుకున్న క్లౌడ్ నిల్వ ద్వారా సింక్ అవుతాయి. పరికరం విడిచే ముందు అన్నీ ఎన్‌క్రిప్ట్ అవుతాయి. మీరు మాత్రమే చదవగలరు.",
    rustCore: "Rust core",
    rustCoreSub: "లైబ్రరీ · సింక్ · ఎన్‌క్రిప్షన్",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "లైబ్రరీ"
      ],
      [
        "నిల్వ",
        "సింక్"
      ]
    ],
    playbackEyebrow: "ప్లేబ్యాక్",
    playbackTitle: "మీ సంగీతం ఎక్కడైనా",
    minis: [
      [
        "మీ సంగీతం",
        "మీ ఫైల్‌లు మీదే"
      ],
      [
        "నిల్వ",
        "మీ పరికరాలు మీరు ఎంచుకున్న క్లౌడ్ నిల్వ ద్వారా సింక్ అవుతాయి. పరికరం విడిచే ముందు అన్నీ ఎన్‌క్రిప్ట్ అవుతాయి. మీరు మాత్రమే చదవగలరు."
      ],
      [
        "సింక్",
        "మీ cloud ద్వారా sync"
      ]
    ],
    endAria: "నిర్మాణం చదవండి",
    endPrefix: "నిర్మాణం చదవండి",
    endFallback: ".",
    endText: "ప్రైవేట్, open source, అభివృద్ధిలో ఉంది. మీరు జోడించిన ప్రతిదీ మీదే.",
    readArchitecture: "నిర్మాణం చదవండి",
    footerStatus: "ప్రైవేట్ మరియు open source",
    cycleWords: [
      "ఇంటికి",
      "ఎక్కడైనా",
      "కలిసి",
      "ఆఫ్‌లైన్",
      "నియంత్రణలో"
    ]
  },
  mr: {
    metaDescription: "bae: तुमचे संगीत सर्वत्र. तुमचे सर्व संगीत फोन आणि डेस्कटॉपवर. तुमच्या क्लाउड साठवणीतून डिव्हाइसदरम्यान ऐका. चालवायचा सर्व्हर नाही.",
    title: "bae: तुमचे संगीत सर्वत्र",
    navDocs: "दस्तऐवज",
    navDownload: "डाउनलोड",
    languageLabel: "भाषा",
    heroTitle: [
      "तुमचे संगीत",
      "सर्वत्र"
    ],
    heroText: "तुमचे सर्व संगीत फोन आणि डेस्कटॉपवर. तुमच्या क्लाउड साठवणीतून डिव्हाइसदरम्यान ऐका. चालवायचा सर्व्हर नाही.",
    heroBold: "खाजगी, तुमचे.",
    statusLabel: "bae विकास स्थिती",
    statusKicker: "Pre-1.0",
    statusTitle: "सामान्य वापरासाठी तयार नाही.",
    statusText: "फक्त चाचणी build. डेटा आणि sync स्वरूप migration शिवाय बदलतील.",
    downloadMac: "macOS साठी डाउनलोड",
    seeFeatures: "वैशिष्ट्ये पहा",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "तुमचे संगीत",
        "तुमच्या फाइल्स तुमच्याच राहतात"
      ],
      [
        "खाजगी",
        "तुमची डिव्हाइस कळा ठेवतात"
      ],
      [
        "सर्वत्र",
        "तुमच्या cloud मधून sync"
      ]
    ],
    sheepAlt: "तुमचे संगीत",
    noteSynced: "समक्रमण",
    noteSyncedSub: "तुमच्या डिव्हाइसदरम्यान",
    noteCloud: "तुमचा cloud",
    noteCloudSub: "server नाही",
    syncEyebrow: "कूटबद्ध sync",
    syncTitle: [
      "तुमचे संगीत",
      "सर्वत्र"
    ],
    syncText: "तुमची डिव्हाइसेस तुम्ही निवडलेल्या क्लाउड साठवणीतून समक्रमित होतात. सर्व काही डिव्हाइस सोडण्याआधी कूटबद्ध होते. फक्त तुम्ही वाचू शकता.",
    desktop: "डेस्कटॉप",
    phone: "फोन",
    holdsKeys: "कळा ठेवते",
    encrypted: "कूटबद्धीकरण",
    cloudStorage: "साठवण",
    encryptionLink: "कूटबद्धीकरण →",
    libraryEyebrow: "लायब्ररी",
    libraryTitle: [
      "तुमचा पूर्ण संग्रह,",
      "रिलीजप्रमाणे अचूक"
    ],
    libraryText: "तुमचे अल्बम एका लायब्ररीत ठेवा आणि कुठेही घेऊन जा.",
    cards: [
      [
        "रिलीजप्रमाणे अचूक",
        "तुमचे अल्बम एका लायब्ररीत ठेवा आणि कुठेही घेऊन जा.",
        [
          "मेटाडेटा",
          "आढावा",
          "समक्रमण"
        ]
      ],
      [
        "लायब्ररी",
        "तुमची डिव्हाइसेस तुम्ही निवडलेल्या क्लाउड साठवणीतून समक्रमित होतात. सर्व काही डिव्हाइस सोडण्याआधी कूटबद्ध होते. फक्त तुम्ही वाचू शकता.",
        [
          "फोन",
          "डेस्कटॉप"
        ]
      ],
      [
        "कूटबद्धीकरण",
        "तुमची डिव्हाइसेस तुम्ही निवडलेल्या क्लाउड साठवणीतून समक्रमित होतात. सर्व काही डिव्हाइस सोडण्याआधी कूटबद्ध होते. फक्त तुम्ही वाचू शकता.",
        [
          "कूटबद्धीकरण",
          "समक्रमण"
        ]
      ],
      [
        "तुमचे संगीत",
        "तुमच्या फाइल्स तुमच्याच राहतात",
        [
          "तुमचे संगीत",
          "खाजगी आणि open source"
        ]
      ]
    ],
    engineEyebrow: "रचना",
    engineTitle: [
      "डेस्कटॉप",
      "फोन"
    ],
    engineText: "तुमची डिव्हाइसेस तुम्ही निवडलेल्या क्लाउड साठवणीतून समक्रमित होतात. सर्व काही डिव्हाइस सोडण्याआधी कूटबद्ध होते. फक्त तुम्ही वाचू शकता.",
    rustCore: "Rust core",
    rustCoreSub: "लायब्ररी · समक्रमण · कूटबद्धीकरण",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "लायब्ररी"
      ],
      [
        "साठवण",
        "समक्रमण"
      ]
    ],
    playbackEyebrow: "प्लेबॅक",
    playbackTitle: "तुमचे संगीत सर्वत्र",
    minis: [
      [
        "तुमचे संगीत",
        "तुमच्या फाइल्स तुमच्याच राहतात"
      ],
      [
        "साठवण",
        "तुमची डिव्हाइसेस तुम्ही निवडलेल्या क्लाउड साठवणीतून समक्रमित होतात. सर्व काही डिव्हाइस सोडण्याआधी कूटबद्ध होते. फक्त तुम्ही वाचू शकता."
      ],
      [
        "समक्रमण",
        "तुमच्या cloud मधून sync"
      ]
    ],
    endAria: "रचना वाचा",
    endPrefix: "रचना वाचा",
    endFallback: ".",
    endText: "खाजगी, open source आणि विकासात. तुम्ही जोडलेले सर्व तुमचेच राहते.",
    readArchitecture: "रचना वाचा",
    footerStatus: "खाजगी आणि open source",
    cycleWords: [
      "घरी",
      "सर्वत्र",
      "एकत्र",
      "ऑफलाइन",
      "नियंत्रणात"
    ]
  },
  ur: {
    metaDescription: "bae: آپ کی موسیقی ہر جگہ. آپ کی ساری موسیقی فون اور ڈیسک ٹاپ پر۔ اپنے کلاؤڈ اسٹوریج کے ذریعے آلات کے درمیان سنیں۔ چلانے کے لیے کوئی سرور نہیں۔",
    title: "bae: آپ کی موسیقی ہر جگہ",
    navDocs: "دستاویزات",
    navDownload: "ڈاؤن لوڈ",
    languageLabel: "زبان",
    heroTitle: [
      "آپ کی موسیقی",
      "ہر جگہ"
    ],
    heroText: "آپ کی ساری موسیقی فون اور ڈیسک ٹاپ پر۔ اپنے کلاؤڈ اسٹوریج کے ذریعے آلات کے درمیان سنیں۔ چلانے کے لیے کوئی سرور نہیں۔",
    heroBold: "نجی، آپ کی۔",
    statusLabel: "bae کی ترقی کی حالت",
    statusKicker: "Pre-1.0",
    statusTitle: "عام استعمال کے لیے تیار نہیں۔",
    statusText: "صرف ٹیسٹ build۔ ڈیٹا اور sync فارمیٹ migration کے بغیر بدلیں گے۔",
    downloadMac: "macOS کے لیے ڈاؤن لوڈ",
    seeFeatures: "خصوصیات دیکھیں",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "آپ کی موسیقی",
        "آپ کی فائلیں آپ کی رہتی ہیں"
      ],
      [
        "نجی",
        "آپ کے آلات کلیدیں رکھتے ہیں"
      ],
      [
        "ہر جگہ",
        "آپ کے cloud سے sync"
      ]
    ],
    sheepAlt: "آپ کی موسیقی",
    noteSynced: "مطابقت پذیری",
    noteSyncedSub: "آپ کے آلات کے درمیان",
    noteCloud: "آپ کا cloud",
    noteCloudSub: "server نہیں",
    syncEyebrow: "خفیہ sync",
    syncTitle: [
      "آپ کی موسیقی",
      "ہر جگہ"
    ],
    syncText: "آپ کے آلات آپ کے منتخب کردہ کلاؤڈ اسٹوریج سے مطابقت پذیر ہوتے ہیں۔ ہر چیز آلے سے نکلنے سے پہلے خفیہ ہو جاتی ہے۔ صرف آپ پڑھ سکتے ہیں۔",
    desktop: "ڈیسک ٹاپ",
    phone: "فون",
    holdsKeys: "کلیدیں رکھتا ہے",
    encrypted: "خفیہ کاری",
    cloudStorage: "اسٹوریج",
    encryptionLink: "خفیہ کاری →",
    libraryEyebrow: "لائبریری",
    libraryTitle: [
      "آپ کا پورا مجموعہ،",
      "ریلیز کے مطابق درست"
    ],
    libraryText: "اپنے البمز کو ایک لائبریری میں رکھیں اور ہر جگہ لے جائیں۔",
    cards: [
      [
        "ریلیز کے مطابق درست",
        "اپنے البمز کو ایک لائبریری میں رکھیں اور ہر جگہ لے جائیں۔",
        [
          "میٹا ڈیٹا",
          "جائزہ",
          "مطابقت پذیری"
        ]
      ],
      [
        "لائبریری",
        "آپ کے آلات آپ کے منتخب کردہ کلاؤڈ اسٹوریج سے مطابقت پذیر ہوتے ہیں۔ ہر چیز آلے سے نکلنے سے پہلے خفیہ ہو جاتی ہے۔ صرف آپ پڑھ سکتے ہیں۔",
        [
          "فون",
          "ڈیسک ٹاپ"
        ]
      ],
      [
        "خفیہ کاری",
        "آپ کے آلات آپ کے منتخب کردہ کلاؤڈ اسٹوریج سے مطابقت پذیر ہوتے ہیں۔ ہر چیز آلے سے نکلنے سے پہلے خفیہ ہو جاتی ہے۔ صرف آپ پڑھ سکتے ہیں۔",
        [
          "خفیہ کاری",
          "مطابقت پذیری"
        ]
      ],
      [
        "آپ کی موسیقی",
        "آپ کی فائلیں آپ کی رہتی ہیں",
        [
          "آپ کی موسیقی",
          "نجی اور open source"
        ]
      ]
    ],
    engineEyebrow: "ساخت",
    engineTitle: [
      "ڈیسک ٹاپ",
      "فون"
    ],
    engineText: "آپ کے آلات آپ کے منتخب کردہ کلاؤڈ اسٹوریج سے مطابقت پذیر ہوتے ہیں۔ ہر چیز آلے سے نکلنے سے پہلے خفیہ ہو جاتی ہے۔ صرف آپ پڑھ سکتے ہیں۔",
    rustCore: "Rust core",
    rustCoreSub: "لائبریری · مطابقت پذیری · خفیہ کاری",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "لائبریری"
      ],
      [
        "اسٹوریج",
        "مطابقت پذیری"
      ]
    ],
    playbackEyebrow: "پلے بیک",
    playbackTitle: "آپ کی موسیقی ہر جگہ",
    minis: [
      [
        "آپ کی موسیقی",
        "آپ کی فائلیں آپ کی رہتی ہیں"
      ],
      [
        "اسٹوریج",
        "آپ کے آلات آپ کے منتخب کردہ کلاؤڈ اسٹوریج سے مطابقت پذیر ہوتے ہیں۔ ہر چیز آلے سے نکلنے سے پہلے خفیہ ہو جاتی ہے۔ صرف آپ پڑھ سکتے ہیں۔"
      ],
      [
        "مطابقت پذیری",
        "آپ کے cloud سے sync"
      ]
    ],
    endAria: "ساخت پڑھیں",
    endPrefix: "ساخت پڑھیں",
    endFallback: ".",
    endText: "نجی، open source، اور ترقی میں۔ آپ جو کچھ شامل کرتے ہیں وہ آپ کا رہتا ہے۔",
    readArchitecture: "ساخت پڑھیں",
    footerStatus: "نجی اور open source",
    cycleWords: [
      "گھر",
      "ہر جگہ",
      "ساتھ",
      "آف لائن",
      "اختیار میں"
    ]
  },
  gu: {
    metaDescription: "bae: તમારું સંગીત બધે. તમારું બધું સંગીત ફોન અને ડેસ્કટોપ પર. તમારા ક્લાઉડ સંગ્રહ દ્વારા ઉપકરણો વચ્ચે સાંભળો. ચલાવવાનો સર્વર નથી.",
    title: "bae: તમારું સંગીત બધે",
    navDocs: "દસ્તાવેજો",
    navDownload: "ડાઉનલોડ",
    languageLabel: "ભાષા",
    heroTitle: [
      "તમારું સંગીત",
      "બધે"
    ],
    heroText: "તમારું બધું સંગીત ફોન અને ડેસ્કટોપ પર. તમારા ક્લાઉડ સંગ્રહ દ્વારા ઉપકરણો વચ્ચે સાંભળો. ચલાવવાનો સર્વર નથી.",
    heroBold: "ખાનગી, તમારું.",
    statusLabel: "bae વિકાસ સ્થિતિ",
    statusKicker: "Pre-1.0",
    statusTitle: "સામાન્ય ઉપયોગ માટે તૈયાર નથી.",
    statusText: "ફક્ત પરીક્ષણ build. ડેટા અને sync ફોર્મેટ migration વગર બદલાશે.",
    downloadMac: "macOS માટે ડાઉનલોડ",
    seeFeatures: "સુવિધાઓ જુઓ",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "તમારું સંગીત",
        "તમારી ફાઇલો તમારી જ રહે છે"
      ],
      [
        "ખાનગી",
        "તમારા ઉપકરણો કીઓ રાખે છે"
      ],
      [
        "બધે",
        "તમારા cloud દ્વારા sync"
      ]
    ],
    sheepAlt: "તમારું સંગીત",
    noteSynced: "સિંક",
    noteSyncedSub: "તમારા ઉપકરણો વચ્ચે",
    noteCloud: "તમારો cloud",
    noteCloudSub: "server નથી",
    syncEyebrow: "એન્ક્રિપ્ટેડ sync",
    syncTitle: [
      "તમારું સંગીત",
      "બધે"
    ],
    syncText: "તમારા ઉપકરણો તમે પસંદ કરેલા ક્લાઉડ સંગ્રહ દ્વારા સિંક થાય છે. બધું ઉપકરણ છોડે તે પહેલાં એન્ક્રિપ્ટ થાય છે. માત્ર તમે વાંચી શકો છો.",
    desktop: "ડેસ્કટોપ",
    phone: "ફોન",
    holdsKeys: "કીઓ રાખે છે",
    encrypted: "એન્ક્રિપ્શન",
    cloudStorage: "સંગ્રહ",
    encryptionLink: "એન્ક્રિપ્શન →",
    libraryEyebrow: "લાઇબ્રેરી",
    libraryTitle: [
      "તમારો સંપૂર્ણ સંગ્રહ,",
      "રિલીઝ મુજબ ચોક્કસ"
    ],
    libraryText: "તમારા એલ્બમોને એક લાઇબ્રેરીમાં મૂકો અને બધે લઈ જાઓ.",
    cards: [
      [
        "રિલીઝ મુજબ ચોક્કસ",
        "તમારા એલ્બમોને એક લાઇબ્રેરીમાં મૂકો અને બધે લઈ જાઓ.",
        [
          "મેટાડેટા",
          "ઝાંખી",
          "સિંક"
        ]
      ],
      [
        "લાઇબ્રેરી",
        "તમારા ઉપકરણો તમે પસંદ કરેલા ક્લાઉડ સંગ્રહ દ્વારા સિંક થાય છે. બધું ઉપકરણ છોડે તે પહેલાં એન્ક્રિપ્ટ થાય છે. માત્ર તમે વાંચી શકો છો.",
        [
          "ફોન",
          "ડેસ્કટોપ"
        ]
      ],
      [
        "એન્ક્રિપ્શન",
        "તમારા ઉપકરણો તમે પસંદ કરેલા ક્લાઉડ સંગ્રહ દ્વારા સિંક થાય છે. બધું ઉપકરણ છોડે તે પહેલાં એન્ક્રિપ્ટ થાય છે. માત્ર તમે વાંચી શકો છો.",
        [
          "એન્ક્રિપ્શન",
          "સિંક"
        ]
      ],
      [
        "તમારું સંગીત",
        "તમારી ફાઇલો તમારી જ રહે છે",
        [
          "તમારું સંગીત",
          "ખાનગી અને open source"
        ]
      ]
    ],
    engineEyebrow: "રચના",
    engineTitle: [
      "ડેસ્કટોપ",
      "ફોન"
    ],
    engineText: "તમારા ઉપકરણો તમે પસંદ કરેલા ક્લાઉડ સંગ્રહ દ્વારા સિંક થાય છે. બધું ઉપકરણ છોડે તે પહેલાં એન્ક્રિપ્ટ થાય છે. માત્ર તમે વાંચી શકો છો.",
    rustCore: "Rust core",
    rustCoreSub: "લાઇબ્રેરી · સિંક · એન્ક્રિપ્શન",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "લાઇબ્રેરી"
      ],
      [
        "સંગ્રહ",
        "સિંક"
      ]
    ],
    playbackEyebrow: "પ્લેબેક",
    playbackTitle: "તમારું સંગીત બધે",
    minis: [
      [
        "તમારું સંગીત",
        "તમારી ફાઇલો તમારી જ રહે છે"
      ],
      [
        "સંગ્રહ",
        "તમારા ઉપકરણો તમે પસંદ કરેલા ક્લાઉડ સંગ્રહ દ્વારા સિંક થાય છે. બધું ઉપકરણ છોડે તે પહેલાં એન્ક્રિપ્ટ થાય છે. માત્ર તમે વાંચી શકો છો."
      ],
      [
        "સિંક",
        "તમારા cloud દ્વારા sync"
      ]
    ],
    endAria: "રચના વાંચો",
    endPrefix: "રચના વાંચો",
    endFallback: ".",
    endText: "ખાનગી, open source અને વિકાસમાં. તમે ઉમેરો તે બધું તમારું જ રહે છે.",
    readArchitecture: "રચના વાંચો",
    footerStatus: "ખાનગી અને open source",
    cycleWords: [
      "ઘરે",
      "બધે",
      "સાથે",
      "ઑફલાઇન",
      "નિયંત્રણમાં"
    ]
  },
  kn: {
    metaDescription: "bae: ನಿಮ್ಮ ಸಂಗೀತ ಎಲ್ಲೆಡೆ. ನಿಮ್ಮ ಎಲ್ಲಾ ಸಂಗೀತ ಫೋನ್ ಮತ್ತು ಡೆಸ್ಕ್‌ಟಾಪ್‌ನಲ್ಲಿ. ನಿಮ್ಮ ಕ್ಲೌಡ್ ಸಂಗ್ರಹಣೆಯ ಮೂಲಕ ಸಾಧನಗಳ ನಡುವೆ ಕೇಳಿ. ಚಾಲನೆ ಮಾಡಬೇಕಾದ ಸರ್ವರ್ ಇಲ್ಲ.",
    title: "bae: ನಿಮ್ಮ ಸಂಗೀತ ಎಲ್ಲೆಡೆ",
    navDocs: "ದಾಖಲೆಗಳು",
    navDownload: "ಡೌನ್‌ಲೋಡ್",
    languageLabel: "ಭಾಷೆ",
    heroTitle: [
      "ನಿಮ್ಮ ಸಂಗೀತ",
      "ಎಲ್ಲೆಡೆ"
    ],
    heroText: "ನಿಮ್ಮ ಎಲ್ಲಾ ಸಂಗೀತ ಫೋನ್ ಮತ್ತು ಡೆಸ್ಕ್‌ಟಾಪ್‌ನಲ್ಲಿ. ನಿಮ್ಮ ಕ್ಲೌಡ್ ಸಂಗ್ರಹಣೆಯ ಮೂಲಕ ಸಾಧನಗಳ ನಡುವೆ ಕೇಳಿ. ಚಾಲನೆ ಮಾಡಬೇಕಾದ ಸರ್ವರ್ ಇಲ್ಲ.",
    heroBold: "ಖಾಸಗಿ, ನಿಮ್ಮದು.",
    statusLabel: "bae ಅಭಿವೃದ್ಧಿ ಸ್ಥಿತಿ",
    statusKicker: "Pre-1.0",
    statusTitle: "ಸಾಮಾನ್ಯ ಬಳಕೆಗೆ ಸಿದ್ಧವಿಲ್ಲ.",
    statusText: "ಪರೀಕ್ಷಾ build ಮಾತ್ರ. ಡೇಟಾ ಮತ್ತು sync ರೂಪಗಳು migration ಇಲ್ಲದೆ ಬದಲಾಗುತ್ತವೆ.",
    downloadMac: "macOS ಗೆ ಡೌನ್‌ಲೋಡ್",
    seeFeatures: "ವೈಶಿಷ್ಟ್ಯಗಳನ್ನು ನೋಡಿ",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "ನಿಮ್ಮ ಸಂಗೀತ",
        "ನಿಮ್ಮ ಫೈಲ್‌ಗಳು ನಿಮ್ಮದೇ ಇರುತ್ತವೆ"
      ],
      [
        "ಖಾಸಗಿ",
        "ನಿಮ್ಮ ಸಾಧನಗಳು ಕೀಲಿಗಳನ್ನು ಇಟ್ಟುಕೊಳ್ಳುತ್ತವೆ"
      ],
      [
        "ಎಲ್ಲೆಡೆ",
        "ನಿಮ್ಮ cloud ಮೂಲಕ sync"
      ]
    ],
    sheepAlt: "ನಿಮ್ಮ ಸಂಗೀತ",
    noteSynced: "ಸಿಂಕ್",
    noteSyncedSub: "ನಿಮ್ಮ ಸಾಧನಗಳ ನಡುವೆ",
    noteCloud: "ನಿಮ್ಮ cloud",
    noteCloudSub: "server ಇಲ್ಲ",
    syncEyebrow: "ಎನ್‌ಕ್ರಿಪ್ಟ್ ಮಾಡಿದ sync",
    syncTitle: [
      "ನಿಮ್ಮ ಸಂಗೀತ",
      "ಎಲ್ಲೆಡೆ"
    ],
    syncText: "ನಿಮ್ಮ ಸಾಧನಗಳು ನೀವು ಆಯ್ದ ಕ್ಲೌಡ್ ಸಂಗ್ರಹಣೆಯ ಮೂಲಕ ಸಿಂಕ್ ಆಗುತ್ತವೆ. ಸಾಧನ ತೊರೆಯುವ ಮೊದಲು ಎಲ್ಲವೂ ಎನ್‌ಕ್ರಿಪ್ಟ್ ಆಗುತ್ತದೆ. ನೀವು ಮಾತ್ರ ಓದಬಹುದು.",
    desktop: "ಡೆಸ್ಕ್‌ಟಾಪ್",
    phone: "ಫೋನ್",
    holdsKeys: "ಕೀಲಿಗಳನ್ನು ಇಟ್ಟುಕೊಳ್ಳುತ್ತದೆ",
    encrypted: "ಎನ್‌ಕ್ರಿಪ್ಷನ್",
    cloudStorage: "ಸಂಗ್ರಹಣೆ",
    encryptionLink: "ಎನ್‌ಕ್ರಿಪ್ಷನ್ →",
    libraryEyebrow: "ಲೈಬ್ರರಿ",
    libraryTitle: [
      "ನಿಮ್ಮ ಸಂಪೂರ್ಣ ಸಂಗ್ರಹ,",
      "ರಿಲೀಸ್‌ಗೆ ಸರಿಯಾಗಿ"
    ],
    libraryText: "ನಿಮ್ಮ ಆಲ್ಬಂಗಳನ್ನು ಒಂದು ಲೈಬ್ರರಿಯಲ್ಲಿ ಇಟ್ಟು ಎಲ್ಲಿಗೆ ಬೇಕಾದರೂ ತೆಗೆದುಕೊಂಡು ಹೋಗಿ.",
    cards: [
      [
        "ರಿಲೀಸ್‌ಗೆ ಸರಿಯಾಗಿ",
        "ನಿಮ್ಮ ಆಲ್ಬಂಗಳನ್ನು ಒಂದು ಲೈಬ್ರರಿಯಲ್ಲಿ ಇಟ್ಟು ಎಲ್ಲಿಗೆ ಬೇಕಾದರೂ ತೆಗೆದುಕೊಂಡು ಹೋಗಿ.",
        [
          "ಮೆಟಾಡೇಟಾ",
          "ಅವಲೋಕನ",
          "ಸಿಂಕ್"
        ]
      ],
      [
        "ಲೈಬ್ರರಿ",
        "ನಿಮ್ಮ ಸಾಧನಗಳು ನೀವು ಆಯ್ದ ಕ್ಲೌಡ್ ಸಂಗ್ರಹಣೆಯ ಮೂಲಕ ಸಿಂಕ್ ಆಗುತ್ತವೆ. ಸಾಧನ ತೊರೆಯುವ ಮೊದಲು ಎಲ್ಲವೂ ಎನ್‌ಕ್ರಿಪ್ಟ್ ಆಗುತ್ತದೆ. ನೀವು ಮಾತ್ರ ಓದಬಹುದು.",
        [
          "ಫೋನ್",
          "ಡೆಸ್ಕ್‌ಟಾಪ್"
        ]
      ],
      [
        "ಎನ್‌ಕ್ರಿಪ್ಷನ್",
        "ನಿಮ್ಮ ಸಾಧನಗಳು ನೀವು ಆಯ್ದ ಕ್ಲೌಡ್ ಸಂಗ್ರಹಣೆಯ ಮೂಲಕ ಸಿಂಕ್ ಆಗುತ್ತವೆ. ಸಾಧನ ತೊರೆಯುವ ಮೊದಲು ಎಲ್ಲವೂ ಎನ್‌ಕ್ರಿಪ್ಟ್ ಆಗುತ್ತದೆ. ನೀವು ಮಾತ್ರ ಓದಬಹುದು.",
        [
          "ಎನ್‌ಕ್ರಿಪ್ಷನ್",
          "ಸಿಂಕ್"
        ]
      ],
      [
        "ನಿಮ್ಮ ಸಂಗೀತ",
        "ನಿಮ್ಮ ಫೈಲ್‌ಗಳು ನಿಮ್ಮದೇ ಇರುತ್ತವೆ",
        [
          "ನಿಮ್ಮ ಸಂಗೀತ",
          "ಖಾಸಗಿ ಮತ್ತು open source"
        ]
      ]
    ],
    engineEyebrow: "ವಾಸ್ತುಶಿಲ್ಪ",
    engineTitle: [
      "ಡೆಸ್ಕ್‌ಟಾಪ್",
      "ಫೋನ್"
    ],
    engineText: "ನಿಮ್ಮ ಸಾಧನಗಳು ನೀವು ಆಯ್ದ ಕ್ಲೌಡ್ ಸಂಗ್ರಹಣೆಯ ಮೂಲಕ ಸಿಂಕ್ ಆಗುತ್ತವೆ. ಸಾಧನ ತೊರೆಯುವ ಮೊದಲು ಎಲ್ಲವೂ ಎನ್‌ಕ್ರಿಪ್ಟ್ ಆಗುತ್ತದೆ. ನೀವು ಮಾತ್ರ ಓದಬಹುದು.",
    rustCore: "Rust core",
    rustCoreSub: "ಲೈಬ್ರರಿ · ಸಿಂಕ್ · ಎನ್‌ಕ್ರಿಪ್ಷನ್",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "ಲೈಬ್ರರಿ"
      ],
      [
        "ಸಂಗ್ರಹಣೆ",
        "ಸಿಂಕ್"
      ]
    ],
    playbackEyebrow: "ಪ್ಲೇಬ್ಯಾಕ್",
    playbackTitle: "ನಿಮ್ಮ ಸಂಗೀತ ಎಲ್ಲೆಡೆ",
    minis: [
      [
        "ನಿಮ್ಮ ಸಂಗೀತ",
        "ನಿಮ್ಮ ಫೈಲ್‌ಗಳು ನಿಮ್ಮದೇ ಇರುತ್ತವೆ"
      ],
      [
        "ಸಂಗ್ರಹಣೆ",
        "ನಿಮ್ಮ ಸಾಧನಗಳು ನೀವು ಆಯ್ದ ಕ್ಲೌಡ್ ಸಂಗ್ರಹಣೆಯ ಮೂಲಕ ಸಿಂಕ್ ಆಗುತ್ತವೆ. ಸಾಧನ ತೊರೆಯುವ ಮೊದಲು ಎಲ್ಲವೂ ಎನ್‌ಕ್ರಿಪ್ಟ್ ಆಗುತ್ತದೆ. ನೀವು ಮಾತ್ರ ಓದಬಹುದು."
      ],
      [
        "ಸಿಂಕ್",
        "ನಿಮ್ಮ cloud ಮೂಲಕ sync"
      ]
    ],
    endAria: "ವಾಸ್ತುಶಿಲ್ಪ ಓದಿ",
    endPrefix: "ವಾಸ್ತುಶಿಲ್ಪ ಓದಿ",
    endFallback: ".",
    endText: "ಖಾಸಗಿ, open source, ಮತ್ತು ಅಭಿವೃದ್ಧಿಯಲ್ಲಿದೆ. ನೀವು ಸೇರಿಸುವುದೆಲ್ಲ ನಿಮ್ಮದೇ ಇರುತ್ತದೆ.",
    readArchitecture: "ವಾಸ್ತುಶಿಲ್ಪ ಓದಿ",
    footerStatus: "ಖಾಸಗಿ ಮತ್ತು open source",
    cycleWords: [
      "ಮನೆಗೆ",
      "ಎಲ್ಲೆಡೆ",
      "ಒಟ್ಟಿಗೆ",
      "ಆಫ್‌ಲೈನ್",
      "ನಿಯಂತ್ರಣದಲ್ಲಿ"
    ]
  },
  ml: {
    metaDescription: "bae: നിങ്ങളുടെ സംഗീതം എല്ലായിടത്തും. നിങ്ങളുടെ മുഴുവൻ സംഗീതവും ഫോൺ, ഡെസ്ക്ടോപ്പ് എന്നിവയിൽ. നിങ്ങളുടെ ക്ലൗഡ് സംഭരണത്തിലൂടെ ഉപകരണങ്ങൾക്കിടയിൽ കേൾക്കൂ. പ്രവർത്തിപ്പിക്കേണ്ട സർവർ ഇല്ല.",
    title: "bae: നിങ്ങളുടെ സംഗീതം എല്ലായിടത്തും",
    navDocs: "ഡോക്സ്",
    navDownload: "ഡൗൺലോഡ്",
    languageLabel: "ഭാഷ",
    heroTitle: [
      "നിങ്ങളുടെ സംഗീതം",
      "എല്ലായിടത്തും"
    ],
    heroText: "നിങ്ങളുടെ മുഴുവൻ സംഗീതവും ഫോൺ, ഡെസ്ക്ടോപ്പ് എന്നിവയിൽ. നിങ്ങളുടെ ക്ലൗഡ് സംഭരണത്തിലൂടെ ഉപകരണങ്ങൾക്കിടയിൽ കേൾക്കൂ. പ്രവർത്തിപ്പിക്കേണ്ട സർവർ ഇല്ല.",
    heroBold: "സ്വകാര്യവും നിങ്ങളുടേതും.",
    statusLabel: "bae വികസന നില",
    statusKicker: "Pre-1.0",
    statusTitle: "പൊതു ഉപയോഗത്തിന് തയ്യാറല്ല.",
    statusText: "പരിശോധന build മാത്രം. ഡാറ്റയും sync രൂപങ്ങളും migration ഇല്ലാതെ മാറും.",
    downloadMac: "macOS-നായി ഡൗൺലോഡ്",
    seeFeatures: "സവിശേഷതകൾ കാണുക",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "നിങ്ങളുടെ സംഗീതം",
        "നിങ്ങളുടെ ഫയലുകൾ നിങ്ങളുടേതായി തുടരും"
      ],
      [
        "സ്വകാര്യ",
        "നിങ്ങളുടെ ഉപകരണങ്ങൾ കീകൾ സൂക്ഷിക്കുന്നു"
      ],
      [
        "എല്ലായിടത്തും",
        "നിങ്ങളുടെ cloud വഴി sync"
      ]
    ],
    sheepAlt: "നിങ്ങളുടെ സംഗീതം",
    noteSynced: "സിങ്ക്",
    noteSyncedSub: "നിങ്ങളുടെ ഉപകരണങ്ങൾക്കിടയിൽ",
    noteCloud: "നിങ്ങളുടെ cloud",
    noteCloudSub: "server ഇല്ല",
    syncEyebrow: "എൻക്രിപ്റ്റ് ചെയ്ത sync",
    syncTitle: [
      "നിങ്ങളുടെ സംഗീതം",
      "എല്ലായിടത്തും"
    ],
    syncText: "നിങ്ങളുടെ ഉപകരണങ്ങൾ നിങ്ങൾ തെരഞ്ഞെടുത്ത ക്ലൗഡ് സംഭരണത്തിലൂടെ സിങ്ക് ചെയ്യുന്നു. ഉപകരണത്തെ വിട്ടുപോകുന്നതിന് മുമ്പ് എല്ലാം എൻക്രിപ്റ്റ് ചെയ്യപ്പെടുന്നു. നിങ്ങൾക്കു മാത്രമേ വായിക്കാനാകൂ.",
    desktop: "ഡെസ്ക്ടോപ്പ്",
    phone: "ഫോൺ",
    holdsKeys: "കീകൾ സൂക്ഷിക്കുന്നു",
    encrypted: "എൻക്രിപ്ഷൻ",
    cloudStorage: "സംഭരണം",
    encryptionLink: "എൻക്രിപ്ഷൻ →",
    libraryEyebrow: "ലൈബ്രറി",
    libraryTitle: [
      "നിങ്ങളുടെ മുഴുവൻ ശേഖരം,",
      "റിലീസ് കൃത്യതയോടെ"
    ],
    libraryText: "നിങ്ങളുടെ ആൽബങ്ങൾ ഒരു ലൈബ്രറിയിലേക്ക് കൊണ്ടുവന്ന് എവിടെയും കൊണ്ടുപോകൂ.",
    cards: [
      [
        "റിലീസ് കൃത്യതയോടെ",
        "നിങ്ങളുടെ ആൽബങ്ങൾ ഒരു ലൈബ്രറിയിലേക്ക് കൊണ്ടുവന്ന് എവിടെയും കൊണ്ടുപോകൂ.",
        [
          "മെറ്റാഡാറ്റ",
          "അവലോകനം",
          "സിങ്ക്"
        ]
      ],
      [
        "ലൈബ്രറി",
        "നിങ്ങളുടെ ഉപകരണങ്ങൾ നിങ്ങൾ തെരഞ്ഞെടുത്ത ക്ലൗഡ് സംഭരണത്തിലൂടെ സിങ്ക് ചെയ്യുന്നു. ഉപകരണത്തെ വിട്ടുപോകുന്നതിന് മുമ്പ് എല്ലാം എൻക്രിപ്റ്റ് ചെയ്യപ്പെടുന്നു. നിങ്ങൾക്കു മാത്രമേ വായിക്കാനാകൂ.",
        [
          "ഫോൺ",
          "ഡെസ്ക്ടോപ്പ്"
        ]
      ],
      [
        "എൻക്രിപ്ഷൻ",
        "നിങ്ങളുടെ ഉപകരണങ്ങൾ നിങ്ങൾ തെരഞ്ഞെടുത്ത ക്ലൗഡ് സംഭരണത്തിലൂടെ സിങ്ക് ചെയ്യുന്നു. ഉപകരണത്തെ വിട്ടുപോകുന്നതിന് മുമ്പ് എല്ലാം എൻക്രിപ്റ്റ് ചെയ്യപ്പെടുന്നു. നിങ്ങൾക്കു മാത്രമേ വായിക്കാനാകൂ.",
        [
          "എൻക്രിപ്ഷൻ",
          "സിങ്ക്"
        ]
      ],
      [
        "നിങ്ങളുടെ സംഗീതം",
        "നിങ്ങളുടെ ഫയലുകൾ നിങ്ങളുടേതായി തുടരും",
        [
          "നിങ്ങളുടെ സംഗീതം",
          "സ്വകാര്യവും open source-ഉം"
        ]
      ]
    ],
    engineEyebrow: "ആർക്കിടെക്ചർ",
    engineTitle: [
      "ഡെസ്ക്ടോപ്പ്",
      "ഫോൺ"
    ],
    engineText: "നിങ്ങളുടെ ഉപകരണങ്ങൾ നിങ്ങൾ തെരഞ്ഞെടുത്ത ക്ലൗഡ് സംഭരണത്തിലൂടെ സിങ്ക് ചെയ്യുന്നു. ഉപകരണത്തെ വിട്ടുപോകുന്നതിന് മുമ്പ് എല്ലാം എൻക്രിപ്റ്റ് ചെയ്യപ്പെടുന്നു. നിങ്ങൾക്കു മാത്രമേ വായിക്കാനാകൂ.",
    rustCore: "Rust core",
    rustCoreSub: "ലൈബ്രറി · സിങ്ക് · എൻക്രിപ്ഷൻ",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "ലൈബ്രറി"
      ],
      [
        "സംഭരണം",
        "സിങ്ക്"
      ]
    ],
    playbackEyebrow: "പ്ലേബാക്ക്",
    playbackTitle: "നിങ്ങളുടെ സംഗീതം എല്ലായിടത്തും",
    minis: [
      [
        "നിങ്ങളുടെ സംഗീതം",
        "നിങ്ങളുടെ ഫയലുകൾ നിങ്ങളുടേതായി തുടരും"
      ],
      [
        "സംഭരണം",
        "നിങ്ങളുടെ ഉപകരണങ്ങൾ നിങ്ങൾ തെരഞ്ഞെടുത്ത ക്ലൗഡ് സംഭരണത്തിലൂടെ സിങ്ക് ചെയ്യുന്നു. ഉപകരണത്തെ വിട്ടുപോകുന്നതിന് മുമ്പ് എല്ലാം എൻക്രിപ്റ്റ് ചെയ്യപ്പെടുന്നു. നിങ്ങൾക്കു മാത്രമേ വായിക്കാനാകൂ."
      ],
      [
        "സിങ്ക്",
        "നിങ്ങളുടെ cloud വഴി sync"
      ]
    ],
    endAria: "ആർക്കിടെക്ചർ വായിക്കുക",
    endPrefix: "ആർക്കിടെക്ചർ വായിക്കുക",
    endFallback: ".",
    endText: "സ്വകാര്യവും open source-ഉം വികസനത്തിലുമാണ്. നിങ്ങൾ ചേർക്കുന്നതെല്ലാം നിങ്ങളുടേതായി തുടരും.",
    readArchitecture: "ആർക്കിടെക്ചർ വായിക്കുക",
    footerStatus: "സ്വകാര്യവും open source-ഉം",
    cycleWords: [
      "വീട്ടിലേക്ക്",
      "എവിടെയും",
      "ഒരുമിച്ച്",
      "ഓഫ്‌ലൈൻ",
      "നിയന്ത്രണത്തിൽ"
    ]
  },
  pa: {
    metaDescription: "bae: ਤੁਹਾਡਾ ਸੰਗੀਤ ਹਰ ਜਗ੍ਹਾ. ਤੁਹਾਡਾ ਸਾਰਾ ਸੰਗੀਤ ਫੋਨ ਅਤੇ ਡੈਸਕਟਾਪ ਉੱਤੇ। ਆਪਣੇ ਕਲਾਉਡ ਸਟੋਰੇਜ ਰਾਹੀਂ ਡਿਵਾਈਸਾਂ ਵਿਚਕਾਰ ਸੁਣੋ। ਚਲਾਉਣ ਲਈ ਕੋਈ ਸਰਵਰ ਨਹੀਂ।",
    title: "bae: ਤੁਹਾਡਾ ਸੰਗੀਤ ਹਰ ਜਗ੍ਹਾ",
    navDocs: "ਡਾਕਸ",
    navDownload: "ਡਾਊਨਲੋਡ",
    languageLabel: "ਭਾਸ਼ਾ",
    heroTitle: [
      "ਤੁਹਾਡਾ ਸੰਗੀਤ",
      "ਹਰ ਜਗ੍ਹਾ"
    ],
    heroText: "ਤੁਹਾਡਾ ਸਾਰਾ ਸੰਗੀਤ ਫੋਨ ਅਤੇ ਡੈਸਕਟਾਪ ਉੱਤੇ। ਆਪਣੇ ਕਲਾਉਡ ਸਟੋਰੇਜ ਰਾਹੀਂ ਡਿਵਾਈਸਾਂ ਵਿਚਕਾਰ ਸੁਣੋ। ਚਲਾਉਣ ਲਈ ਕੋਈ ਸਰਵਰ ਨਹੀਂ।",
    heroBold: "ਨਿੱਜੀ, ਤੁਹਾਡਾ।",
    statusLabel: "bae ਵਿਕਾਸ ਹਾਲਤ",
    statusKicker: "Pre-1.0",
    statusTitle: "ਆਮ ਵਰਤੋਂ ਲਈ ਤਿਆਰ ਨਹੀਂ।",
    statusText: "ਕੇਵਲ ਟੈਸਟ build। ਡਾਟਾ ਅਤੇ sync ਫਾਰਮੈਟ migration ਤੋਂ ਬਿਨਾਂ ਬਦਲਣਗੇ।",
    downloadMac: "macOS ਲਈ ਡਾਊਨਲੋਡ",
    seeFeatures: "ਫੀਚਰ ਵੇਖੋ",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "ਤੁਹਾਡਾ ਸੰਗੀਤ",
        "ਤੁਹਾਡੀਆਂ ਫਾਈਲਾਂ ਤੁਹਾਡੀਆਂ ਰਹਿੰਦੀਆਂ ਹਨ"
      ],
      [
        "ਨਿੱਜੀ",
        "ਤੁਹਾਡੇ ਡਿਵਾਈਸ ਕੁੰਜੀਆਂ ਰੱਖਦੇ ਹਨ"
      ],
      [
        "ਹਰ ਜਗ੍ਹਾ",
        "ਤੁਹਾਡੇ cloud ਰਾਹੀਂ sync"
      ]
    ],
    sheepAlt: "ਤੁਹਾਡਾ ਸੰਗੀਤ",
    noteSynced: "ਸਿੰਕ",
    noteSyncedSub: "ਤੁਹਾਡੇ ਡਿਵਾਈਸਾਂ ਵਿਚਕਾਰ",
    noteCloud: "ਤੁਹਾਡਾ cloud",
    noteCloudSub: "server ਨਹੀਂ",
    syncEyebrow: "ਇਨਕ੍ਰਿਪਟਿਡ sync",
    syncTitle: [
      "ਤੁਹਾਡਾ ਸੰਗੀਤ",
      "ਹਰ ਜਗ੍ਹਾ"
    ],
    syncText: "ਤੁਹਾਡੇ ਡਿਵਾਈਸ ਤੁਹਾਡੇ ਚੁਣੇ ਕਲਾਉਡ ਸਟੋਰੇਜ ਰਾਹੀਂ ਸਿੰਕ ਹੁੰਦੇ ਹਨ। ਸਭ ਕੁਝ ਡਿਵਾਈਸ ਛੱਡਣ ਤੋਂ ਪਹਿਲਾਂ ਇਨਕ੍ਰਿਪਟ ਹੁੰਦਾ ਹੈ। ਸਿਰਫ਼ ਤੁਸੀਂ ਪੜ੍ਹ ਸਕਦੇ ਹੋ।",
    desktop: "ਡੈਸਕਟਾਪ",
    phone: "ਫੋਨ",
    holdsKeys: "ਕੁੰਜੀਆਂ ਰੱਖਦਾ ਹੈ",
    encrypted: "ਇਨਕ੍ਰਿਪਸ਼ਨ",
    cloudStorage: "ਸਟੋਰੇਜ",
    encryptionLink: "ਇਨਕ੍ਰਿਪਸ਼ਨ →",
    libraryEyebrow: "ਲਾਇਬ੍ਰੇਰੀ",
    libraryTitle: [
      "ਤੁਹਾਡਾ ਪੂਰਾ ਸੰਗ੍ਰਹਿ,",
      "ਰਿਲੀਜ਼ ਮੁਤਾਬਕ ਸਹੀ"
    ],
    libraryText: "ਆਪਣੇ ਐਲਬਮ ਇੱਕ ਲਾਇਬ੍ਰੇਰੀ ਵਿੱਚ ਰੱਖੋ ਅਤੇ ਹਰ ਥਾਂ ਲੈ ਜਾਓ।",
    cards: [
      [
        "ਰਿਲੀਜ਼ ਮੁਤਾਬਕ ਸਹੀ",
        "ਆਪਣੇ ਐਲਬਮ ਇੱਕ ਲਾਇਬ੍ਰੇਰੀ ਵਿੱਚ ਰੱਖੋ ਅਤੇ ਹਰ ਥਾਂ ਲੈ ਜਾਓ।",
        [
          "ਮੈਟਾਡਾਟਾ",
          "ਝਲਕ",
          "ਸਿੰਕ"
        ]
      ],
      [
        "ਲਾਇਬ੍ਰੇਰੀ",
        "ਤੁਹਾਡੇ ਡਿਵਾਈਸ ਤੁਹਾਡੇ ਚੁਣੇ ਕਲਾਉਡ ਸਟੋਰੇਜ ਰਾਹੀਂ ਸਿੰਕ ਹੁੰਦੇ ਹਨ। ਸਭ ਕੁਝ ਡਿਵਾਈਸ ਛੱਡਣ ਤੋਂ ਪਹਿਲਾਂ ਇਨਕ੍ਰਿਪਟ ਹੁੰਦਾ ਹੈ। ਸਿਰਫ਼ ਤੁਸੀਂ ਪੜ੍ਹ ਸਕਦੇ ਹੋ।",
        [
          "ਫੋਨ",
          "ਡੈਸਕਟਾਪ"
        ]
      ],
      [
        "ਇਨਕ੍ਰਿਪਸ਼ਨ",
        "ਤੁਹਾਡੇ ਡਿਵਾਈਸ ਤੁਹਾਡੇ ਚੁਣੇ ਕਲਾਉਡ ਸਟੋਰੇਜ ਰਾਹੀਂ ਸਿੰਕ ਹੁੰਦੇ ਹਨ। ਸਭ ਕੁਝ ਡਿਵਾਈਸ ਛੱਡਣ ਤੋਂ ਪਹਿਲਾਂ ਇਨਕ੍ਰਿਪਟ ਹੁੰਦਾ ਹੈ। ਸਿਰਫ਼ ਤੁਸੀਂ ਪੜ੍ਹ ਸਕਦੇ ਹੋ।",
        [
          "ਇਨਕ੍ਰਿਪਸ਼ਨ",
          "ਸਿੰਕ"
        ]
      ],
      [
        "ਤੁਹਾਡਾ ਸੰਗੀਤ",
        "ਤੁਹਾਡੀਆਂ ਫਾਈਲਾਂ ਤੁਹਾਡੀਆਂ ਰਹਿੰਦੀਆਂ ਹਨ",
        [
          "ਤੁਹਾਡਾ ਸੰਗੀਤ",
          "ਨਿੱਜੀ ਅਤੇ open source"
        ]
      ]
    ],
    engineEyebrow: "ਆਰਕੀਟੈਕਚਰ",
    engineTitle: [
      "ਡੈਸਕਟਾਪ",
      "ਫੋਨ"
    ],
    engineText: "ਤੁਹਾਡੇ ਡਿਵਾਈਸ ਤੁਹਾਡੇ ਚੁਣੇ ਕਲਾਉਡ ਸਟੋਰੇਜ ਰਾਹੀਂ ਸਿੰਕ ਹੁੰਦੇ ਹਨ। ਸਭ ਕੁਝ ਡਿਵਾਈਸ ਛੱਡਣ ਤੋਂ ਪਹਿਲਾਂ ਇਨਕ੍ਰਿਪਟ ਹੁੰਦਾ ਹੈ। ਸਿਰਫ਼ ਤੁਸੀਂ ਪੜ੍ਹ ਸਕਦੇ ਹੋ।",
    rustCore: "Rust core",
    rustCoreSub: "ਲਾਇਬ੍ਰੇਰੀ · ਸਿੰਕ · ਇਨਕ੍ਰਿਪਸ਼ਨ",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "ਲਾਇਬ੍ਰੇਰੀ"
      ],
      [
        "ਸਟੋਰੇਜ",
        "ਸਿੰਕ"
      ]
    ],
    playbackEyebrow: "ਪਲੇਬੈਕ",
    playbackTitle: "ਤੁਹਾਡਾ ਸੰਗੀਤ ਹਰ ਜਗ੍ਹਾ",
    minis: [
      [
        "ਤੁਹਾਡਾ ਸੰਗੀਤ",
        "ਤੁਹਾਡੀਆਂ ਫਾਈਲਾਂ ਤੁਹਾਡੀਆਂ ਰਹਿੰਦੀਆਂ ਹਨ"
      ],
      [
        "ਸਟੋਰੇਜ",
        "ਤੁਹਾਡੇ ਡਿਵਾਈਸ ਤੁਹਾਡੇ ਚੁਣੇ ਕਲਾਉਡ ਸਟੋਰੇਜ ਰਾਹੀਂ ਸਿੰਕ ਹੁੰਦੇ ਹਨ। ਸਭ ਕੁਝ ਡਿਵਾਈਸ ਛੱਡਣ ਤੋਂ ਪਹਿਲਾਂ ਇਨਕ੍ਰਿਪਟ ਹੁੰਦਾ ਹੈ। ਸਿਰਫ਼ ਤੁਸੀਂ ਪੜ੍ਹ ਸਕਦੇ ਹੋ।"
      ],
      [
        "ਸਿੰਕ",
        "ਤੁਹਾਡੇ cloud ਰਾਹੀਂ sync"
      ]
    ],
    endAria: "ਆਰਕੀਟੈਕਚਰ ਪੜ੍ਹੋ",
    endPrefix: "ਆਰਕੀਟੈਕਚਰ ਪੜ੍ਹੋ",
    endFallback: ".",
    endText: "ਨਿੱਜੀ, open source ਅਤੇ ਵਿਕਾਸ ਵਿੱਚ। ਤੁਸੀਂ ਜੋ ਜੋੜਦੇ ਹੋ ਉਹ ਤੁਹਾਡਾ ਰਹਿੰਦਾ ਹੈ।",
    readArchitecture: "ਆਰਕੀਟੈਕਚਰ ਪੜ੍ਹੋ",
    footerStatus: "ਨਿੱਜੀ ਅਤੇ open source",
    cycleWords: [
      "ਘਰ",
      "ਹਰ ਥਾਂ",
      "ਇਕੱਠੇ",
      "ਆਫਲਾਈਨ",
      "ਕੰਟਰੋਲ ਵਿੱਚ"
    ]
  },
  th: {
    metaDescription: "bae: เพลงของคุณทุกที่. เพลงทั้งหมดของคุณบนโทรศัพท์และเดสก์ท็อป ฟังข้ามอุปกรณ์ผ่านพื้นที่จัดเก็บคลาวด์ของคุณเอง ไม่มีเซิร์ฟเวอร์ที่ต้องดูแล",
    title: "bae: เพลงของคุณทุกที่",
    navDocs: "เอกสาร",
    navDownload: "ดาวน์โหลด",
    languageLabel: "ภาษา",
    heroTitle: [
      "เพลงของคุณ",
      "ทุกที่"
    ],
    heroText: "เพลงทั้งหมดของคุณบนโทรศัพท์และเดสก์ท็อป ฟังข้ามอุปกรณ์ผ่านพื้นที่จัดเก็บคลาวด์ของคุณเอง ไม่มีเซิร์ฟเวอร์ที่ต้องดูแล",
    heroBold: "ส่วนตัว เป็นของคุณ",
    statusLabel: "สถานะการพัฒนา bae",
    statusKicker: "Pre-1.0",
    statusTitle: "ยังไม่พร้อมสำหรับการใช้งานทั่วไป",
    statusText: "มีเฉพาะ build สำหรับทดสอบ รูปแบบข้อมูลและ sync จะเปลี่ยนโดยไม่มี migration",
    downloadMac: "ดาวน์โหลดสำหรับ macOS",
    seeFeatures: "ดูคุณสมบัติ",
    platformMeta: "macOS · iOS · Windows · Android",
    trust: [
      [
        "เพลงของคุณ",
        "ไฟล์ยังเป็นของคุณ"
      ],
      [
        "ส่วนตัว",
        "อุปกรณ์ของคุณเก็บกุญแจ"
      ],
      [
        "ทุกที่",
        "sync ผ่าน cloud ของคุณ"
      ]
    ],
    sheepAlt: "เพลงของคุณ",
    noteSynced: "ซิงค์",
    noteSyncedSub: "ระหว่างอุปกรณ์ของคุณ",
    noteCloud: "cloud ของคุณ",
    noteCloudSub: "ไม่มี server",
    syncEyebrow: "sync แบบเข้ารหัส",
    syncTitle: [
      "เพลงของคุณ",
      "ทุกที่"
    ],
    syncText: "อุปกรณ์ของคุณซิงค์ผ่านพื้นที่จัดเก็บคลาวด์ที่คุณเลือก ทุกอย่างถูกเข้ารหัสก่อนออกจากอุปกรณ์ มีเพียงคุณที่อ่านได้",
    desktop: "เดสก์ท็อป",
    phone: "โทรศัพท์",
    holdsKeys: "เก็บกุญแจ",
    encrypted: "การเข้ารหัส",
    cloudStorage: "พื้นที่จัดเก็บ",
    encryptionLink: "การเข้ารหัส →",
    libraryEyebrow: "ไลบรารี",
    libraryTitle: [
      "คอลเลกชันทั้งหมดของคุณ",
      "ตรงตามรุ่นที่เผยแพร่"
    ],
    libraryText: "นำอัลบั้มที่คุณมีมาไว้ในไลบรารีเดียว แล้วพกไปได้ทุกที่",
    cards: [
      [
        "ตรงตามรุ่นที่เผยแพร่",
        "นำอัลบั้มที่คุณมีมาไว้ในไลบรารีเดียว แล้วพกไปได้ทุกที่",
        [
          "เมทาดาทา",
          "ภาพรวม",
          "ซิงค์"
        ]
      ],
      [
        "ไลบรารี",
        "อุปกรณ์ของคุณซิงค์ผ่านพื้นที่จัดเก็บคลาวด์ที่คุณเลือก ทุกอย่างถูกเข้ารหัสก่อนออกจากอุปกรณ์ มีเพียงคุณที่อ่านได้",
        [
          "โทรศัพท์",
          "เดสก์ท็อป"
        ]
      ],
      [
        "การเข้ารหัส",
        "อุปกรณ์ของคุณซิงค์ผ่านพื้นที่จัดเก็บคลาวด์ที่คุณเลือก ทุกอย่างถูกเข้ารหัสก่อนออกจากอุปกรณ์ มีเพียงคุณที่อ่านได้",
        [
          "การเข้ารหัส",
          "ซิงค์"
        ]
      ],
      [
        "เพลงของคุณ",
        "ไฟล์ยังเป็นของคุณ",
        [
          "เพลงของคุณ",
          "ส่วนตัวและ open source"
        ]
      ]
    ],
    engineEyebrow: "สถาปัตยกรรม",
    engineTitle: [
      "เดสก์ท็อป",
      "โทรศัพท์"
    ],
    engineText: "อุปกรณ์ของคุณซิงค์ผ่านพื้นที่จัดเก็บคลาวด์ที่คุณเลือก ทุกอย่างถูกเข้ารหัสก่อนออกจากอุปกรณ์ มีเพียงคุณที่อ่านได้",
    rustCore: "Rust core",
    rustCoreSub: "ไลบรารี · ซิงค์ · การเข้ารหัส",
    sharedRustCore: "Rust core",
    deps: [
      [
        "FFmpeg",
        "audio"
      ],
      [
        "SQLite",
        "ไลบรารี"
      ],
      [
        "พื้นที่จัดเก็บ",
        "ซิงค์"
      ]
    ],
    playbackEyebrow: "การเล่น",
    playbackTitle: "เพลงของคุณ ทุกที่",
    minis: [
      [
        "เพลงของคุณ",
        "ไฟล์ยังเป็นของคุณ"
      ],
      [
        "พื้นที่จัดเก็บ",
        "อุปกรณ์ของคุณซิงค์ผ่านพื้นที่จัดเก็บคลาวด์ที่คุณเลือก ทุกอย่างถูกเข้ารหัสก่อนออกจากอุปกรณ์ มีเพียงคุณที่อ่านได้"
      ],
      [
        "ซิงค์",
        "sync ผ่าน cloud ของคุณ"
      ]
    ],
    endAria: "อ่านสถาปัตยกรรม",
    endPrefix: "อ่านสถาปัตยกรรม",
    endFallback: ".",
    endText: "เป็นส่วนตัว open source และอยู่ระหว่างพัฒนา ทุกอย่างที่คุณเพิ่มยังเป็นของคุณ",
    readArchitecture: "อ่านสถาปัตยกรรม",
    footerStatus: "ส่วนตัวและ open source",
    cycleWords: [
      "กลับบ้าน",
      "ทุกที่",
      "ด้วยกัน",
      "ออฟไลน์",
      "อยู่ในการควบคุม"
    ]
  }
};

for (const [section, values] of Object.entries(addedSidebarTranslations.sections)) {
  Object.assign(sidebarTranslations.sections[section], values);
}
for (const [page, values] of Object.entries(addedSidebarTranslations.pages)) {
  Object.assign(sidebarTranslations.pages[page], values);
}
Object.assign(landing, addedLanding);

const fallback = landing.en;
for (const locale of LOCALES) {
  landing[locale.code] ??= fallback;
}
