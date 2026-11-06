import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const en = {
  nav: {
    start: "Start",
    import: "Import",
    trim: "Trim / QC",
    blast: "BLAST",
    results: "Results",
    export: "Export",
    settings: "Settings",
  },
  common: {
    noProject: "No project open",
    close: "Close project",
    saveAs: "Save as…",
    cancel: "Cancel",
    save: "Save",
    samples: "samples",
  },
  start: {
    title: "COI Barcode BLAST",
    subtitle: "Sanger traces to species: trim, BLAST, compile.",
    newProject: "New project from .zip or .fasta",
    open: "Open project…",
    recent: "Recent projects",
    none: "No recent projects yet.",
    missing: "File not found",
    forget: "Remove from list",
  },
  blast: {
    searchAll: "Search all eligible samples",
    searchChanged: "Search new and changed samples",
    cancel: "Cancel search",
  },
  results: {
    compile: "Compile results",
    recompile: "Recompile needed",
  },
};

const id: typeof en = {
  nav: {
    start: "Mulai",
    import: "Impor",
    trim: "Trim / QC",
    blast: "BLAST",
    results: "Hasil",
    export: "Ekspor",
    settings: "Pengaturan",
  },
  common: {
    noProject: "Belum ada proyek",
    close: "Tutup proyek",
    saveAs: "Simpan sebagai…",
    cancel: "Batal",
    save: "Simpan",
    samples: "sampel",
  },
  start: {
    title: "COI Barcode BLAST",
    subtitle: "Dari kromatogram Sanger ke spesies: trim, BLAST, kompilasi.",
    newProject: "Proyek baru dari .zip atau .fasta",
    open: "Buka proyek…",
    recent: "Proyek terakhir",
    none: "Belum ada proyek.",
    missing: "File tidak ditemukan",
    forget: "Hapus dari daftar",
  },
  blast: {
    searchAll: "Cari semua sampel yang lolos",
    searchChanged: "Cari sampel baru dan yang berubah",
    cancel: "Batalkan pencarian",
  },
  results: {
    compile: "Kompilasi hasil",
    recompile: "Perlu dikompilasi ulang",
  },
};

i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, id: { translation: id } },
  lng: "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
