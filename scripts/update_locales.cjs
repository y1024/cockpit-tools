
const fs = require('fs');
const path = require('path');

const localesDir = path.join(__dirname, '..', 'src', 'locales');

// Full translations including settings.general keys
const translations = {
  ar: {
    codex_oauth_openBrowser: "فتح في المتصفح",
    codex_oauth_hint: "بمجرد التفويض ، سيتم تحديث هذه النافذة تلقائيًا",
    codex_token_import: "استيراد",
    codex_local_import: "الحصول على الحساب المحلي",
    codex_oauth_portInUseAction: "إغلاق المنفذ والمحاولة مرة أخرى",
    codex_opencode_switch: "مفتاح تبديل OpenCode",
    codex_opencode_switch_desc: "مزامنة تبديل معلومات حساب Codex في OpenCode",
    codex_opencode_switch_failed: "فشل تحديث مفتاح OpenCode: {{error}}",
    update_notification_whatsNew: "ما الجديد",
    accounts_confirmDeleteTag: "هل تريد حذف العلامة \"{{tag}}\"؟ ستتم إزالة هذه العلامة من {{count}} حسابات.",
    accounts_defaultGroup: "مجموعة افتراضية",
    settings_general_closeBehavior: "سلوك الإغلاق",
    settings_general_closeBehaviorDesc: "اختر الإجراء عند إغلاق النافذة",
    settings_general_closeBehaviorAsk: "اسأل في كل مرة",
    settings_general_closeBehaviorMinimize: "تصغير إلى الدرج",
    settings_general_closeBehaviorQuit: "إنهاء التطبيق",
    settings_general_opencodeAppPathDesc: "اتركه فارغًا لاستخدام المسار الافتراضي",
    settings_general_dataDir: "دليل البيانات",
    settings_general_dataDirDesc: "موقع تخزين الحسابات وملفات الإعدادات.",
    settings_general_opencodeTitle: "تكامل OpenCode",
    settings_general_opencodeRestart: "إعادة تشغيل OpenCode عند تبديل Codex",
    settings_general_opencodeRestartDesc: "مزامنة معلومات حساب Codex في OpenCode",
    settings_general_opencodeAppPath: "مسار تشغيل OpenCode",
    settings_general_opencodeAppPathPlaceholder: "المسار الافتراضي",
    settings_general_opencodePathReset: "إعادة التعيين إلى الافتراضي",
    accounts_filterTags: "تصفية العلامات",
    accounts_filterTagsCount: "العلامات",
    accounts_noAvailableTags: "لا توجد علامات متاحة",
    accounts_clearFilter: "مسح التصفية",
    accounts_editTags: "تحرير العلامات",
    accounts_groupByTag: "تجميع حسب العلامة",
    accounts_untagged: "غير موسوم",
    codex_filterTags: "تصفية العلامات",
    codex_filterTagsCount: "العلامات",
    codex_noAvailableTags: "لا توجد علامات متاحة",
    codex_clearFilter: "مسح التصفية",
    codex_editTags: "تحرير العلامات",
    codex_import_localDesc: "استيراد حسابات Codex من الجلسات المسجلة محليًا."
  },
  cs: {
    codex_oauth_openBrowser: "Otevřít v prohlížeči",
    codex_oauth_hint: "Po autorizaci se toto okno automaticky aktualizuje",
    codex_token_import: "Importovat",
    codex_local_import: "Získat místní účet",
    codex_oauth_portInUseAction: "Zavřít port a zkusit to znovu",
    codex_opencode_switch: "Přepínač OpenCode",
    codex_opencode_switch_desc: "Synchronizovat přepnutí účtu Codex v OpenCode",
    codex_opencode_switch_failed: "Nepodařilo se aktualizovat přepínač OpenCode: {{error}}",
    update_notification_whatsNew: "Co je nového",
    accounts_confirmDeleteTag: "Smazat štítek \"{{tag}}\"? Tento štítek bude odebrán z {{count}} účtů.",
    accounts_defaultGroup: "Výchozí skupina",
    settings_general_closeBehavior: "Chování při zavírání",
    settings_general_closeBehaviorDesc: "Vyberte akci při zavření okna",
    settings_general_closeBehaviorAsk: "Vždy se zeptat",
    settings_general_closeBehaviorMinimize: "Minimalizovat do lišty",
    settings_general_closeBehaviorQuit: "Ukončit aplikaci",
    settings_general_opencodeAppPathDesc: "Ponechte prázdné pro použití výchozí cesty",
    settings_general_dataDir: "Datový adresář",
    settings_general_dataDirDesc: "Umístění úložiště účtů a konfiguračních souborů.",
    settings_general_opencodeTitle: "Integrace OpenCode",
    settings_general_opencodeRestart: "Restartovat OpenCode při přepnutí Codex",
    settings_general_opencodeRestartDesc: "Synchronizovat informace o účtu Codex v OpenCode",
    settings_general_opencodeAppPath: "Cesta spuštění OpenCode",
    settings_general_opencodeAppPathPlaceholder: "Výchozí cesta",
    settings_general_opencodePathReset: "Obnovit výchozí",
    accounts_filterTags: "Filtrovat štítky",
    accounts_filterTagsCount: "Štítky",
    accounts_noAvailableTags: "Žádné dostupné štítky",
    accounts_clearFilter: "Vymazat filtr",
    accounts_editTags: "Upravit štítky",
    accounts_groupByTag: "Seskupit podle štítku",
    accounts_untagged: "Bez štítku",
    codex_filterTags: "Filtrovat štítky",
    codex_filterTagsCount: "Štítky",
    codex_noAvailableTags: "Žádné dostupné štítky",
    codex_clearFilter: "Vymazat filtr",
    codex_editTags: "Upravit štítky",
    codex_import_localDesc: "Importovat účty Codex z místně přihlášených relací."
  },
  de: {
    codex_oauth_openBrowser: "Im Browser öffnen",
    codex_oauth_hint: "Nach der Autorisierung wird dieses Fenster automatisch aktualisiert",
    codex_token_import: "Importieren",
    codex_local_import: "Lokales Konto abrufen",
    codex_oauth_portInUseAction: "Port schließen und erneut versuchen",
    codex_opencode_switch: "OpenCode-Schalter",
    codex_opencode_switch_desc: "Wechsel des Codex-Kontos in OpenCode synchronisieren",
    codex_opencode_switch_failed: "OpenCode-Schalter konnte nicht aktualisiert werden: {{error}}",
    update_notification_whatsNew: "Was ist neu",
    accounts_confirmDeleteTag: "Tag \"{{tag}}\" löschen? Dieser Tag wird von {{count}} Konten entfernt.",
    accounts_defaultGroup: "Standardgruppe",
    settings_general_closeBehavior: "Verhalten beim Schließen",
    settings_general_closeBehaviorDesc: "Aktion beim Schließen des Fensters wählen",
    settings_general_closeBehaviorAsk: "Jedes Mal fragen",
    settings_general_closeBehaviorMinimize: "In den Tray minimieren",
    settings_general_closeBehaviorQuit: "Anwendung beenden",
    settings_general_opencodeAppPathDesc: "Leer lassen, um den Standardpfad zu verwenden",
    settings_general_dataDir: "Datenverzeichnis",
    settings_general_dataDirDesc: "Speicherort für Konten und Konfigurationsdateien.",
    settings_general_opencodeTitle: "OpenCode-Integration",
    settings_general_opencodeRestart: "OpenCode beim Codex-Wechsel neu starten",
    settings_general_opencodeRestartDesc: "Codex-Kontoinformationen in OpenCode synchronisieren",
    settings_general_opencodeAppPath: "OpenCode-Startpfad",
    settings_general_opencodeAppPathPlaceholder: "Standardpfad",
    settings_general_opencodePathReset: "Standard zurücksetzen",
    accounts_filterTags: "Tags filtern",
    accounts_filterTagsCount: "Tags",
    accounts_noAvailableTags: "Keine verfügbaren Tags",
    accounts_clearFilter: "Filter löschen",
    accounts_editTags: "Tags bearbeiten",
    accounts_groupByTag: "Nach Tag gruppieren",
    accounts_untagged: "Ohne Tag",
    codex_filterTags: "Tags filtern",
    codex_filterTagsCount: "Tags",
    codex_noAvailableTags: "Keine verfügbaren Tags",
    codex_clearFilter: "Filter löschen",
    codex_editTags: "Tags bearbeiten",
    codex_import_localDesc: "Codex-Konten aus lokal angemeldeten Sitzungen importieren."
  },
  "en-US": { // Same as en
    codex_oauth_openBrowser: "Open in Browser",
    codex_oauth_hint: "Once authorized, this window will update automatically",
    codex_token_import: "Import",
    codex_local_import: "Get Local Account",
    codex_oauth_portInUseAction: "Close port and retry",
    codex_opencode_switch: "OpenCode switch toggle",
    codex_opencode_switch_desc: "Sync Codex account info in OpenCode",
    codex_opencode_switch_failed: "Failed to update OpenCode toggle: {{error}}",
    update_notification_whatsNew: "What's New",
    accounts_confirmDeleteTag: "Delete tag \"{{tag}}\"? This tag will be removed from {{count}} accounts.",
    accounts_defaultGroup: "Default Group",
    settings_general_closeBehavior: "Close Behavior",
    settings_general_closeBehaviorDesc: "Choose action when closing window",
    settings_general_closeBehaviorAsk: "Ask every time",
    settings_general_closeBehaviorMinimize: "Minimize to tray",
    settings_general_closeBehaviorQuit: "Quit application",
    settings_general_opencodeAppPathDesc: "Leave blank to use the default path",
    settings_general_dataDir: "Data Directory",
    settings_general_dataDirDesc: "Storage location for accounts and configuration files.",
    settings_general_opencodeTitle: "OpenCode Integration",
    settings_general_opencodeRestart: "Restart OpenCode when switching Codex",
    settings_general_opencodeRestartDesc: "Sync Codex account info in OpenCode",
    settings_general_opencodeAppPath: "OpenCode launch path",
    settings_general_opencodeAppPathPlaceholder: "Default path",
    settings_general_opencodePathReset: "Reset default",
    accounts_filterTags: "Filter Tags",
    accounts_filterTagsCount: "Tags",
    accounts_noAvailableTags: "No available tags",
    accounts_clearFilter: "Clear filter",
    accounts_editTags: "Edit tags",
    accounts_groupByTag: "Group by tag",
    accounts_untagged: "Untagged",
    codex_filterTags: "Filter Tags",
    codex_filterTagsCount: "Tags",
    codex_noAvailableTags: "No available tags",
    codex_clearFilter: "Clear filter",
    codex_editTags: "Edit tags",
    codex_import_localDesc: "Import Codex accounts from locally signed-in sessions."
  },
  es: {
    codex_oauth_openBrowser: "Abrir en el navegador",
    codex_oauth_hint: "Una vez autorizado, esta ventana se actualizará automáticamente",
    codex_token_import: "Importar",
    codex_local_import: "Obtener cuenta local",
    codex_oauth_portInUseAction: "Cerrar puerto y reintentar",
    codex_opencode_switch: "Interruptor de OpenCode",
    codex_opencode_switch_desc: "Sincronizar el cambio de cuenta de Codex en OpenCode",
    codex_opencode_switch_failed: "Error al actualizar el interruptor de OpenCode: {{error}}",
    update_notification_whatsNew: "Novedades",
    accounts_confirmDeleteTag: "¿Eliminar etiqueta \"{{tag}}\"? Esta etiqueta se eliminará de {{count}} cuentas.",
    accounts_defaultGroup: "Grupo predeterminado",
    settings_general_closeBehavior: "Comportamiento al cerrar",
    settings_general_closeBehaviorDesc: "Elegir acción al cerrar la ventana",
    settings_general_closeBehaviorAsk: "Preguntar siempre",
    settings_general_closeBehaviorMinimize: "Minimizar a la bandeja",
    settings_general_closeBehaviorQuit: "Salir de la aplicación",
    settings_general_opencodeAppPathDesc: "Deja en blanco para usar la ruta predeterminada",
    settings_general_dataDir: "Directorio de datos",
    settings_general_dataDirDesc: "Ubicación de almacenamiento para cuentas y archivos de configuración.",
    settings_general_opencodeTitle: "Integración de OpenCode",
    settings_general_opencodeRestart: "Reiniciar OpenCode al cambiar Codex",
    settings_general_opencodeRestartDesc: "Sincronizar la información de la cuenta de Codex en OpenCode",
    settings_general_opencodeAppPath: "Ruta de inicio de OpenCode",
    settings_general_opencodeAppPathPlaceholder: "Ruta predeterminada",
    settings_general_opencodePathReset: "Restablecer predeterminado",
    accounts_filterTags: "Filtrar etiquetas",
    accounts_filterTagsCount: "Etiquetas",
    accounts_noAvailableTags: "No hay etiquetas disponibles",
    accounts_clearFilter: "Limpiar filtro",
    accounts_editTags: "Editar etiquetas",
    accounts_groupByTag: "Agrupar por etiqueta",
    accounts_untagged: "Sin etiqueta",
    codex_filterTags: "Filtrar etiquetas",
    codex_filterTagsCount: "Etiquetas",
    codex_noAvailableTags: "No hay etiquetas disponibles",
    codex_clearFilter: "Limpiar filtro",
    codex_editTags: "Editar etiquetas",
    codex_import_localDesc: "Importar cuentas de Codex desde sesiones iniciadas localmente."
  },
  fr: {
    codex_oauth_openBrowser: "Ouvrir dans le navigateur",
    codex_oauth_hint: "Une fois autorisé, cette fenêtre se mettra à jour automatiquement",
    codex_token_import: "Importer",
    codex_local_import: "Obtener le compte local",
    codex_oauth_portInUseAction: "Fermer le port et réessayer",
    codex_opencode_switch: "Interrupteur OpenCode",
    codex_opencode_switch_desc: "Synchroniser le changement de compte Codex dans OpenCode",
    codex_opencode_switch_failed: "Échec de la mise à jour de l'interrupteur OpenCode : {{error}}",
    update_notification_whatsNew: "Nouveautés",
    accounts_confirmDeleteTag: "Supprimer l'étiquette \"{{tag}}\" ? Cette étiquette sera supprimée de {{count}} comptes.",
    accounts_defaultGroup: "Groupe par défaut",
    settings_general_closeBehavior: "Comportement à la fermeture",
    settings_general_closeBehaviorDesc: "Action à la fermeture de la fenêtre",
    settings_general_closeBehaviorAsk: "Demander à chaque fois",
    settings_general_closeBehaviorMinimize: "Minimiser dans la barre d'état",
    settings_general_closeBehaviorQuit: "Quitter l'application",
    settings_general_opencodeAppPathDesc: "Laisser vide pour utiliser le chemin par défaut",
    settings_general_dataDir: "Répertoire de données",
    settings_general_dataDirDesc: "Emplacement de stockage des comptes et fichiers de configuration.",
    settings_general_opencodeTitle: "Intégration OpenCode",
    settings_general_opencodeRestart: "Redémarrer OpenCode lors du changement de Codex",
    settings_general_opencodeRestartDesc: "Synchroniser les informations du compte Codex dans OpenCode",
    settings_general_opencodeAppPath: "Chemin de lancement OpenCode",
    settings_general_opencodeAppPathPlaceholder: "Chemin par défaut",
    settings_general_opencodePathReset: "Réinitialiser par défaut",
    accounts_filterTags: "Filtrer les tags",
    accounts_filterTagsCount: "Tags",
    accounts_noAvailableTags: "Aucun tag disponible",
    accounts_clearFilter: "Effacer le filtre",
    accounts_editTags: "Modifier les tags",
    accounts_groupByTag: "Grouper par tag",
    accounts_untagged: "Sans tag",
    codex_filterTags: "Filtrer les tags",
    codex_filterTagsCount: "Tags",
    codex_noAvailableTags: "Aucun tag disponible",
    codex_clearFilter: "Effacer le filtre",
    codex_editTags: "Modifier les tags",
    codex_import_localDesc: "Importer des comptes Codex depuis des sessions locales connectées."
  },
  it: {
    codex_oauth_openBrowser: "Apri nel browser",
    codex_oauth_hint: "Una volta autorizzato, questa finestra si aggiornerà automaticamente",
    codex_token_import: "Importa",
    codex_local_import: "Ottieni account locale",
    codex_oauth_portInUseAction: "Chiudi porta e riprova",
    codex_opencode_switch: "Interruttore OpenCode",
    codex_opencode_switch_desc: "Sincronizza il cambio dell'account Codex in OpenCode",
    codex_opencode_switch_failed: "Impossibile aggiornare l'interruttore OpenCode: {{error}}",
    update_notification_whatsNew: "Novità",
    accounts_confirmDeleteTag: "Eliminare il tag \"{{tag}}\"? Questo tag verrà rimosso da {{count}} account.",
    accounts_defaultGroup: "Gruppo predefinito",
    settings_general_closeBehavior: "Comportamento alla chiusura",
    settings_general_closeBehaviorDesc: "Scegli azione alla chiusura della finestra",
    settings_general_closeBehaviorAsk: "Chiedi ogni volta",
    settings_general_closeBehaviorMinimize: "Riduci a icona nel vassoio",
    settings_general_closeBehaviorQuit: "Esci dall'applicazione",
    settings_general_opencodeAppPathDesc: "Lascia vuoto per usare il percorso predefinito",
    settings_general_dataDir: "Directory dati",
    settings_general_dataDirDesc: "Posizione di archiviazione per account e file di configurazione.",
    settings_general_opencodeTitle: "Integrazione OpenCode",
    settings_general_opencodeRestart: "Riavvia OpenCode quando si cambia Codex",
    settings_general_opencodeRestartDesc: "Sincronizza le informazioni dell'account Codex in OpenCode",
    settings_general_opencodeAppPath: "Percorso di avvio OpenCode",
    settings_general_opencodeAppPathPlaceholder: "Percorso predefinito",
    settings_general_opencodePathReset: "Ripristina predefinito",
    accounts_filterTags: "Filtra tag",
    accounts_filterTagsCount: "Tag",
    accounts_noAvailableTags: "Nessun tag disponibile",
    accounts_clearFilter: "Cancella filtro",
    accounts_editTags: "Modifica tag",
    accounts_groupByTag: "Raggruppa per tag",
    accounts_untagged: "Senza tag",
    codex_filterTags: "Filtra tag",
    codex_filterTagsCount: "Tag",
    codex_noAvailableTags: "Nessun tag disponibile",
    codex_clearFilter: "Cancella filtro",
    codex_editTags: "Modifica tag",
    codex_import_localDesc: "Importa account Codex da sessioni locali con accesso."
  },
  ja: {
    codex_oauth_openBrowser: "ブラウザで開く",
    codex_oauth_hint: "認証が完了すると、このウィンドウは自動的に更新されます",
    codex_token_import: "インポート",
    codex_local_import: "ローカルアカウントを取得",
    codex_oauth_portInUseAction: "ポートを閉じて再試行",
    codex_opencode_switch: "OpenCode 切り替えスイッチ",
    codex_opencode_switch_desc: "OpenCode 内の Codex アカウント情報を同期して切り替え",
    codex_opencode_switch_failed: "OpenCode のスイッチ更新に失敗しました: {{error}}",
    update_notification_whatsNew: "新着情報",
    accounts_confirmDeleteTag: "タグ「{{tag}}」を削除しますか？このタグは {{count}} 個のアカウントから削除されます。",
    accounts_defaultGroup: "デフォルトグループ",
    settings_general_closeBehavior: "閉じる時の動作",
    settings_general_closeBehaviorDesc: "ウィンドウを閉じる時の動作を選択",
    settings_general_closeBehaviorAsk: "毎回確認する",
    settings_general_closeBehaviorMinimize: "トレイに最小化",
    settings_general_closeBehaviorQuit: "アプリを終了",
    settings_general_opencodeAppPathDesc: "空欄の場合は既定のパスを使用",
    settings_general_dataDir: "データディレクトリ",
    settings_general_dataDirDesc: "アカウントと設定ファイルの保存場所。",
    settings_general_opencodeTitle: "OpenCode 連携",
    settings_general_opencodeRestart: "Codex 切替時に OpenCode を自動再起動",
    settings_general_opencodeRestartDesc: "OpenCode 内の Codex アカウント情報を同期して切り替え",
    settings_general_opencodeAppPath: "OpenCode 起動パス",
    settings_general_opencodeAppPathPlaceholder: "既定のパス",
    settings_general_opencodePathReset: "既定に戻す",
    accounts_filterTags: "タグで絞り込み",
    accounts_filterTagsCount: "タグ",
    accounts_noAvailableTags: "利用可能なタグはありません",
    accounts_clearFilter: "フィルターをクリア",
    accounts_editTags: "タグを編集",
    accounts_groupByTag: "タグでグループ化",
    accounts_untagged: "タグなし",
    codex_filterTags: "タグで絞り込み",
    codex_filterTagsCount: "タグ",
    codex_noAvailableTags: "利用可能なタグはありません",
    codex_clearFilter: "フィルターをクリア",
    codex_editTags: "タグを編集",
    codex_import_localDesc: "ローカルでログイン済みのセッションから Codex アカウントをインポート。"
  },
  ko: {
    codex_oauth_openBrowser: "브라우저에서 열기",
    codex_oauth_hint: "승인되면 이 창이 자동으로 업데이트됩니다",
    codex_token_import: "가져오기",
    codex_local_import: "로컬 계정 가져오기",
    codex_oauth_portInUseAction: "포트 닫기 및 재시도",
    codex_opencode_switch: "OpenCode 전환 스위치",
    codex_opencode_switch_desc: "OpenCode에서 Codex 계정 정보를 동기화하여 전환",
    codex_opencode_switch_failed: "OpenCode 스위치 업데이트 실패: {{error}}",
    update_notification_whatsNew: "새로운 기능",
    accounts_confirmDeleteTag: "\"{{tag}}\" 태그를 삭제하시겠습니까? 이 태그는 {{count}}개의 계정에서 제거됩니다.",
    accounts_defaultGroup: "기본 그룹",
    settings_general_closeBehavior: "닫기 동작",
    settings_general_closeBehaviorDesc: "창을 닫을 때의 동작 선택",
    settings_general_closeBehaviorAsk: "항상 묻기",
    settings_general_closeBehaviorMinimize: "트레이로 최소화",
    settings_general_closeBehaviorQuit: "애플리케이션 종료",
    settings_general_opencodeAppPathDesc: "비워 두면 기본 경로 사용",
    settings_general_dataDir: "데이터 디렉터리",
    settings_general_dataDirDesc: "계정 및 설정 파일의 저장 위치입니다.",
    settings_general_opencodeTitle: "OpenCode 연동",
    settings_general_opencodeRestart: "Codex 전환 시 OpenCode 재시작",
    settings_general_opencodeRestartDesc: "OpenCode에서 Codex 계정 정보를 동기화하여 전환",
    settings_general_opencodeAppPath: "OpenCode 실행 경로",
    settings_general_opencodeAppPathPlaceholder: "기본 경로",
    settings_general_opencodePathReset: "기본값으로 재설정",
    accounts_filterTags: "태그 필터",
    accounts_filterTagsCount: "태그",
    accounts_noAvailableTags: "사용 가능한 태그가 없습니다",
    accounts_clearFilter: "필터 지우기",
    accounts_editTags: "태그 편집",
    accounts_groupByTag: "태그별 그룹",
    accounts_untagged: "태그 없음",
    codex_filterTags: "태그 필터",
    codex_filterTagsCount: "태그",
    codex_noAvailableTags: "사용 가능한 태그가 없습니다",
    codex_clearFilter: "필터 지우기",
    codex_editTags: "태그 편집",
    codex_import_localDesc: "로컬에 로그인된 세션에서 Codex 계정을 가져오기."
  },
  pl: {
    codex_oauth_openBrowser: "Otwórz w przeglądarce",
    codex_oauth_hint: "Po autoryzacji to okno zaktualizuje się automatycznie",
    codex_token_import: "Importuj",
    codex_local_import: "Pobierz konto lokalne",
    codex_oauth_portInUseAction: "Zamknij port i spróbuj ponownie",
    codex_opencode_switch: "Przełącznik OpenCode",
    codex_opencode_switch_desc: "Synchronizuj przełączenie konta Codex w OpenCode",
    codex_opencode_switch_failed: "Nie udało się zaktualizować przełącznika OpenCode: {{error}}",
    update_notification_whatsNew: "Co nowego",
    accounts_confirmDeleteTag: "Usunąć tag \"{{tag}}\"? Ten tag zostanie usunięty z {{count}} kont.",
    accounts_defaultGroup: "Grupa domyślna",
    settings_general_closeBehavior: "Zachowanie przy zamykaniu",
    settings_general_closeBehaviorDesc: "Wybierz akcję przy zamykaniu okna",
    settings_general_closeBehaviorAsk: "Zawsze pytaj",
    settings_general_closeBehaviorMinimize: "Minimalizuj do zasobnika",
    settings_general_closeBehaviorQuit: "Zamknij aplikację",
    settings_general_opencodeAppPathDesc: "Pozostaw puste, aby użyć domyślnej ścieżki",
    settings_general_dataDir: "Katalog danych",
    settings_general_dataDirDesc: "Miejsce przechowywania kont i plików konfiguracyjnych.",
    settings_general_opencodeTitle: "Integracja OpenCode",
    settings_general_opencodeRestart: "Uruchom ponownie OpenCode przy przełączaniu Codex",
    settings_general_opencodeRestartDesc: "Synchronizuj informacje o koncie Codex w OpenCode",
    settings_general_opencodeAppPath: "Ścieżka uruchamiania OpenCode",
    settings_general_opencodeAppPathPlaceholder: "Domyślna ścieżka",
    settings_general_opencodePathReset: "Przywróć domyślne",
    accounts_filterTags: "Filtruj tagi",
    accounts_filterTagsCount: "Tagi",
    accounts_noAvailableTags: "Brak dostępnych tagów",
    accounts_clearFilter: "Wyczyść filtr",
    accounts_editTags: "Edytuj tagi",
    accounts_groupByTag: "Grupuj według taga",
    accounts_untagged: "Bez taga",
    codex_filterTags: "Filtruj tagi",
    codex_filterTagsCount: "Tagi",
    codex_noAvailableTags: "Brak dostępnych tagów",
    codex_clearFilter: "Wyczyść filtr",
    codex_editTags: "Edytuj tagi",
    codex_import_localDesc: "Importuj konta Codex z lokalnie zalogowanych sesji."
  },
  "pt-br": {
    codex_oauth_openBrowser: "Abrir no navegador",
    codex_oauth_hint: "Uma vez autorizado, esta janela será atualizada automaticamente",
    codex_token_import: "Importar",
    codex_local_import: "Obter conta local",
    codex_oauth_portInUseAction: "Fechar porta e tentar novamente",
    codex_opencode_switch: "Alternador do OpenCode",
    codex_opencode_switch_desc: "Sincronizar a troca de conta do Codex no OpenCode",
    codex_opencode_switch_failed: "Falha ao atualizar o alternador do OpenCode: {{error}}",
    update_notification_whatsNew: "O que há de novo",
    accounts_confirmDeleteTag: "Excluir tag \"{{tag}}\"? Esta tag será removida de {{count}} contas.",
    accounts_defaultGroup: "Grupo padrão",
    settings_general_closeBehavior: "Comportamento ao fechar",
    settings_general_closeBehaviorDesc: "Escolha a ação ao fechar a janela",
    settings_general_closeBehaviorAsk: "Perguntar sempre",
    settings_general_closeBehaviorMinimize: "Minimizar para a bandeja",
    settings_general_closeBehaviorQuit: "Sair do aplicativo",
    settings_general_opencodeAppPathDesc: "Deixe em branco para usar o caminho padrão",
    settings_general_dataDir: "Diretório de dados",
    settings_general_dataDirDesc: "Local de armazenamento de contas e arquivos de configuração.",
    settings_general_opencodeTitle: "Integração OpenCode",
    settings_general_opencodeRestart: "Reiniciar o OpenCode ao trocar o Codex",
    settings_general_opencodeRestartDesc: "Sincronizar informações da conta Codex no OpenCode",
    settings_general_opencodeAppPath: "Caminho de inicialização do OpenCode",
    settings_general_opencodeAppPathPlaceholder: "Caminho padrão",
    settings_general_opencodePathReset: "Restaurar padrão",
    accounts_filterTags: "Filtrar tags",
    accounts_filterTagsCount: "Tags",
    accounts_noAvailableTags: "Nenhuma tag disponível",
    accounts_clearFilter: "Limpar filtro",
    accounts_editTags: "Editar tags",
    accounts_groupByTag: "Agrupar por tag",
    accounts_untagged: "Sem tag",
    codex_filterTags: "Filtrar tags",
    codex_filterTagsCount: "Tags",
    codex_noAvailableTags: "Nenhuma tag disponível",
    codex_clearFilter: "Limpar filtro",
    codex_editTags: "Editar tags",
    codex_import_localDesc: "Importar contas Codex de sessões locais conectadas."
  },
  ru: {
    codex_oauth_openBrowser: "Открыть в браузере",
    codex_oauth_hint: "После авторизации это окно обновится автоматически",
    codex_token_import: "Импорт",
    codex_local_import: "Получить локальный аккаунт",
    codex_oauth_portInUseAction: "Закрыть порт и повторить",
    codex_opencode_switch: "Переключатель OpenCode",
    codex_opencode_switch_desc: "Синхронизировать переключение аккаунта Codex в OpenCode",
    codex_opencode_switch_failed: "Не удалось обновить переключатель OpenCode: {{error}}",
    update_notification_whatsNew: "Что нового",
    accounts_confirmDeleteTag: "Удалить тег \"{{tag}}\"? Этот тег будет удален из {{count}} аккаунтов.",
    accounts_defaultGroup: "Группа по умолчанию",
    settings_general_closeBehavior: "Поведение при закрытии",
    settings_general_closeBehaviorDesc: "Действие при закрытии окна",
    settings_general_closeBehaviorAsk: "Спрашивать каждый раз",
    settings_general_closeBehaviorMinimize: "Свернуть в трей",
    settings_general_closeBehaviorQuit: "Закрыть приложение",
    settings_general_opencodeAppPathDesc: "Оставьте пустым, чтобы использовать путь по умолчанию",
    settings_general_dataDir: "Каталог данных",
    settings_general_dataDirDesc: "Место хранения аккаунтов и файлов конфигурации.",
    settings_general_opencodeTitle: "Интеграция OpenCode",
    settings_general_opencodeRestart: "Перезапускать OpenCode при переключении Codex",
    settings_general_opencodeRestartDesc: "Синхронизировать данные аккаунта Codex в OpenCode",
    settings_general_opencodeAppPath: "Путь запуска OpenCode",
    settings_general_opencodeAppPathPlaceholder: "Путь по умолчанию",
    settings_general_opencodePathReset: "Сбросить по умолчанию",
    accounts_filterTags: "Фильтр по тегам",
    accounts_filterTagsCount: "Теги",
    accounts_noAvailableTags: "Нет доступных тегов",
    accounts_clearFilter: "Очистить фильтр",
    accounts_editTags: "Редактировать теги",
    accounts_groupByTag: "Группировать по тегу",
    accounts_untagged: "Без тега",
    codex_filterTags: "Фильтр по тегам",
    codex_filterTagsCount: "Теги",
    codex_noAvailableTags: "Нет доступных тегов",
    codex_clearFilter: "Очистить фильтр",
    codex_editTags: "Редактировать теги",
    codex_import_localDesc: "Импортировать аккаунты Codex из локальных вошедших сессий."
  },
  tr: {
    codex_oauth_openBrowser: "Tarayıcıda aç",
    codex_oauth_hint: "Yetkilendirildikten sonra bu pencere otomatik olarak güncellenecektir",
    codex_token_import: "İçe aktar",
    codex_local_import: "Yerel Hesabı Al",
    codex_oauth_portInUseAction: "Bağlantı noktasını kapat ve tekrar dene",
    codex_opencode_switch: "OpenCode geçiş anahtarı",
    codex_opencode_switch_desc: "OpenCode içinde Codex hesap geçişini senkronize et",
    codex_opencode_switch_failed: "OpenCode anahtarı güncellenemedi: {{error}}",
    update_notification_whatsNew: "Yenilikler",
    accounts_confirmDeleteTag: "\"{{tag}}\" etiketi silinsin mi? Bu etiket {{count}} hesaptan kaldırılacak.",
    accounts_defaultGroup: "Varsayılan Grup",
    settings_general_closeBehavior: "Kapanış Davranışı",
    settings_general_closeBehaviorDesc: "Pencere kapatıldığında yapılacak işlem",
    settings_general_closeBehaviorAsk: "Her seferinde sor",
    settings_general_closeBehaviorMinimize: "Tepsisine küçült",
    settings_general_closeBehaviorQuit: "Uygulamadan Çık",
    settings_general_opencodeAppPathDesc: "Varsayılan yolu kullanmak için boş bırakın",
    settings_general_dataDir: "Veri dizini",
    settings_general_dataDirDesc: "Hesaplar ve yapılandırma dosyaları için depolama konumu.",
    settings_general_opencodeTitle: "OpenCode Entegrasyonu",
    settings_general_opencodeRestart: "Codex değiştirildiğinde OpenCode'u yeniden başlat",
    settings_general_opencodeRestartDesc: "OpenCode içinde Codex hesap bilgisini senkronize et",
    settings_general_opencodeAppPath: "OpenCode başlatma yolu",
    settings_general_opencodeAppPathPlaceholder: "Varsayılan yol",
    settings_general_opencodePathReset: "Varsayılanı sıfırla",
    accounts_filterTags: "Etiketleri filtrele",
    accounts_filterTagsCount: "Etiketler",
    accounts_noAvailableTags: "Kullanılabilir etiket yok",
    accounts_clearFilter: "Filtreyi temizle",
    accounts_editTags: "Etiketleri düzenle",
    accounts_groupByTag: "Etikete göre grupla",
    accounts_untagged: "Etiketsiz",
    codex_filterTags: "Etiketleri filtrele",
    codex_filterTagsCount: "Etiketler",
    codex_noAvailableTags: "Kullanılabilir etiket yok",
    codex_clearFilter: "Filtreyi temizle",
    codex_editTags: "Etiketleri düzenle",
    codex_import_localDesc: "Yerelde oturum açılmış oturumlardan Codex hesaplarını içe aktar."
  },
  vi: {
    codex_oauth_openBrowser: "Mở trong trình duyệt",
    codex_oauth_hint: "Sau khi được ủy quyền, cửa sổ này sẽ tự động cập nhật",
    codex_token_import: "Nhập",
    codex_local_import: "Lấy tài khoản cục bộ",
    codex_oauth_portInUseAction: "Đóng cổng và thử lại",
    codex_opencode_switch: "Công tắc OpenCode",
    codex_opencode_switch_desc: "Đồng bộ chuyển đổi tài khoản Codex trong OpenCode",
    codex_opencode_switch_failed: "Không thể cập nhật công tắc OpenCode: {{error}}",
    update_notification_whatsNew: "Có gì mới",
    accounts_confirmDeleteTag: "Xóa thẻ \"{{tag}}\"? Thẻ này sẽ bị xóa khỏi {{count}} tài khoản.",
    accounts_defaultGroup: "Nhóm mặc định",
    settings_general_closeBehavior: "Hành động khi đóng",
    settings_general_closeBehaviorDesc: "Chọn hành động khi đóng cửa sổ",
    settings_general_closeBehaviorAsk: "Hỏi mỗi lần",
    settings_general_closeBehaviorMinimize: "Thu nhỏ xuống khay",
    settings_general_closeBehaviorQuit: "Thoát ứng dụng",
    settings_general_opencodeAppPathDesc: "Để trống để dùng đường dẫn mặc định",
    settings_general_dataDir: "Thư mục dữ liệu",
    settings_general_dataDirDesc: "Vị trí lưu trữ tài khoản và tệp cấu hình.",
    settings_general_opencodeTitle: "Tích hợp OpenCode",
    settings_general_opencodeRestart: "Khởi động lại OpenCode khi chuyển Codex",
    settings_general_opencodeRestartDesc: "Đồng bộ thông tin tài khoản Codex trong OpenCode",
    settings_general_opencodeAppPath: "Đường dẫn khởi chạy OpenCode",
    settings_general_opencodeAppPathPlaceholder: "Đường dẫn mặc định",
    settings_general_opencodePathReset: "Đặt lại mặc định",
    accounts_filterTags: "Lọc thẻ",
    accounts_filterTagsCount: "Thẻ",
    accounts_noAvailableTags: "Không có thẻ nào",
    accounts_clearFilter: "Xóa bộ lọc",
    accounts_editTags: "Chỉnh sửa thẻ",
    accounts_groupByTag: "Nhóm theo thẻ",
    accounts_untagged: "Chưa gắn thẻ",
    codex_filterTags: "Lọc thẻ",
    codex_filterTagsCount: "Thẻ",
    codex_noAvailableTags: "Không có thẻ nào",
    codex_clearFilter: "Xóa bộ lọc",
    codex_editTags: "Chỉnh sửa thẻ",
    codex_import_localDesc: "Nhập tài khoản Codex từ các phiên đăng nhập cục bộ."
  },
  "zh-tw": {
    codex_oauth_openBrowser: "在瀏覽器中開啟",
    codex_oauth_hint: "完成授權後，此視窗將自動更新",
    codex_token_import: "匯入",
    codex_local_import: "獲取本機帳號",
    codex_oauth_portInUseAction: "關閉連接埠並重試",
    codex_opencode_switch: "OpenCode 切換開關",
    codex_opencode_switch_desc: "同步切換 OpenCode 裡的 Codex 帳號資訊",
    codex_opencode_switch_failed: "更新 OpenCode 開關失敗：{{error}}",
    update_notification_whatsNew: "更新內容",
    accounts_confirmDeleteTag: "確認刪除標籤 \"{{tag}}\" 嗎？該標籤將從 {{count}} 個帳號中移除。",
    accounts_defaultGroup: "預設分組",
    settings_general_closeBehavior: "視窗關閉行為",
    settings_general_closeBehaviorDesc: "選擇關閉視窗時的預設行為",
    settings_general_closeBehaviorAsk: "每次詢問",
    settings_general_closeBehaviorMinimize: "最小化到系統列",
    settings_general_closeBehaviorQuit: "退出應用程式",
    settings_general_opencodeAppPathDesc: "留空則使用預設路徑",
    settings_general_dataDir: "資料目錄",
    settings_general_dataDirDesc: "帳號與設定檔的儲存位置。",
    settings_general_opencodeTitle: "OpenCode 連動",
    settings_general_opencodeRestart: "切換 Codex 時自動重新啟動 OpenCode",
    settings_general_opencodeRestartDesc: "同步切換 OpenCode 裡的 Codex 帳號資訊",
    settings_general_opencodeAppPath: "OpenCode 啟動路徑",
    settings_general_opencodeAppPathPlaceholder: "預設路徑",
    settings_general_opencodePathReset: "重設預設值",
    accounts_filterTags: "標籤篩選",
    accounts_filterTagsCount: "標籤",
    accounts_noAvailableTags: "目前沒有可用標籤",
    accounts_clearFilter: "清除篩選",
    accounts_editTags: "編輯標籤",
    accounts_groupByTag: "依標籤分組",
    accounts_untagged: "未標記",
    codex_filterTags: "標籤篩選",
    codex_filterTagsCount: "標籤",
    codex_noAvailableTags: "目前沒有可用標籤",
    codex_clearFilter: "清除篩選",
    codex_editTags: "編輯標籤",
    codex_import_localDesc: "從本機已登入的會話匯入 Codex 帳號。"
  }
};

const ignoredFiles = ['en.json', 'zh-CN.json'];

function updateFile(fileName) {
  if (ignoredFiles.includes(fileName)) return;

  const code = fileName.replace('.json', '');
  const trans = translations[code];
  
  // Use en-US fallback if language not found
  const actualTrans = trans || translations['en-US'];
  const enTrans = translations['en-US'];

  const filePath = path.join(localesDir, fileName);
  if (!fs.existsSync(filePath)) return;

  try {
    const content = JSON.parse(fs.readFileSync(filePath, 'utf8'));
    let modified = false;

    // Helper to safely set nested keys
    const setKey = (obj, path, value) => {
      const keys = path.split('.');
      let current = obj;
      for (let i = 0; i < keys.length - 1; i++) {
        if (!current[keys[i]]) current[keys[i]] = {};
        current = current[keys[i]];
      }
      if (!current[keys[keys.length - 1]]) {
        current[keys[keys.length - 1]] = value;
        return true;
      }
      return false;
    };
    const setKeyIfEnglish = (obj, path, value, englishValue) => {
      const keys = path.split('.');
      let current = obj;
      for (let i = 0; i < keys.length - 1; i++) {
        if (!current[keys[i]]) current[keys[i]] = {};
        current = current[keys[i]];
      }
      const lastKey = keys[keys.length - 1];
      const existing = current[lastKey];
      if (!existing || existing === englishValue) {
        current[lastKey] = value;
        return true;
      }
      return false;
    };

    if(setKey(content, 'codex.oauth.openBrowser', actualTrans.codex_oauth_openBrowser)) modified = true;
    if(setKey(content, 'codex.oauth.hint', actualTrans.codex_oauth_hint)) modified = true;
    if(setKey(content, 'codex.token.import', actualTrans.codex_token_import)) modified = true;
    if(setKey(content, 'codex.local.import', actualTrans.codex_local_import)) modified = true;
    if(setKey(content, 'codex.oauth.portInUseAction', actualTrans.codex_oauth_portInUseAction)) modified = true;
    if(setKey(content, 'codex.opencodeSwitch', actualTrans.codex_opencode_switch)) modified = true;
    if(setKey(content, 'codex.opencodeSwitchDesc', actualTrans.codex_opencode_switch_desc)) modified = true;
    if(setKey(content, 'codex.opencodeSwitchFailed', actualTrans.codex_opencode_switch_failed)) modified = true;
    if(setKey(content, 'codex.import.localDesc', actualTrans.codex_import_localDesc)) modified = true;
    if(setKey(content, 'accounts.filterTags', actualTrans.accounts_filterTags)) modified = true;
    if(setKey(content, 'accounts.filterTagsCount', actualTrans.accounts_filterTagsCount)) modified = true;
    if(setKey(content, 'accounts.noAvailableTags', actualTrans.accounts_noAvailableTags)) modified = true;
    if(setKey(content, 'accounts.clearFilter', actualTrans.accounts_clearFilter)) modified = true;
    if(setKey(content, 'accounts.editTags', actualTrans.accounts_editTags)) modified = true;
    if(setKey(content, 'accounts.groupByTag', actualTrans.accounts_groupByTag)) modified = true;
    if(setKey(content, 'accounts.untagged', actualTrans.accounts_untagged)) modified = true;
    if(setKey(content, 'codex.filterTags', actualTrans.codex_filterTags)) modified = true;
    if(setKey(content, 'codex.filterTagsCount', actualTrans.codex_filterTagsCount)) modified = true;
    if(setKey(content, 'codex.noAvailableTags', actualTrans.codex_noAvailableTags)) modified = true;
    if(setKey(content, 'codex.clearFilter', actualTrans.codex_clearFilter)) modified = true;
    if(setKey(content, 'codex.editTags', actualTrans.codex_editTags)) modified = true;
    if(setKey(content, 'update_notification.whatsNew', actualTrans.update_notification_whatsNew)) modified = true;
    if(setKey(content, 'accounts.confirmDeleteTag', actualTrans.accounts_confirmDeleteTag)) modified = true;
    if(setKey(content, 'accounts.defaultGroup', actualTrans.accounts_defaultGroup)) modified = true;
    
    // New Settings keys
    if(setKey(content, 'settings.general.closeBehavior', actualTrans.settings_general_closeBehavior)) modified = true;
    if(setKey(content, 'settings.general.closeBehaviorDesc', actualTrans.settings_general_closeBehaviorDesc)) modified = true;
    if(setKey(content, 'settings.general.closeBehaviorAsk', actualTrans.settings_general_closeBehaviorAsk)) modified = true;
    if(setKey(content, 'settings.general.closeBehaviorMinimize', actualTrans.settings_general_closeBehaviorMinimize)) modified = true;
    if(setKey(content, 'settings.general.closeBehaviorQuit', actualTrans.settings_general_closeBehaviorQuit)) modified = true;
    if(setKey(content, 'settings.general.opencodeAppPathDesc', actualTrans.settings_general_opencodeAppPathDesc)) modified = true;
    if(setKey(content, 'settings.general.opencodeTitle', actualTrans.settings_general_opencodeTitle)) modified = true;
    if(setKey(content, 'settings.general.opencodeRestart', actualTrans.settings_general_opencodeRestart)) modified = true;
    if(setKey(content, 'settings.general.opencodeRestartDesc', actualTrans.settings_general_opencodeRestartDesc)) modified = true;
    if(setKey(content, 'settings.general.opencodeAppPath', actualTrans.settings_general_opencodeAppPath)) modified = true;
    if(setKey(content, 'settings.general.opencodeAppPathPlaceholder', actualTrans.settings_general_opencodeAppPathPlaceholder)) modified = true;
    if(setKey(content, 'settings.general.opencodePathReset', actualTrans.settings_general_opencodePathReset)) modified = true;
    if(setKeyIfEnglish(content, 'settings.general.dataDir', actualTrans.settings_general_dataDir, enTrans.settings_general_dataDir)) modified = true;
    if(setKeyIfEnglish(content, 'settings.general.dataDirDesc', actualTrans.settings_general_dataDirDesc, enTrans.settings_general_dataDirDesc)) modified = true;

    if (modified) {
      fs.writeFileSync(filePath, JSON.stringify(content, null, 2));
      console.log(`Updated ${fileName}`);
    } else {
      console.log(`No changes needed for ${fileName}`);
    }

  } catch (e) {
    console.error(`Error updating ${fileName}:`, e);
  }
}

const files = fs.readdirSync(localesDir);
files.forEach(file => {
  if (file.endsWith('.json')) {
    updateFile(file);
  }
});
