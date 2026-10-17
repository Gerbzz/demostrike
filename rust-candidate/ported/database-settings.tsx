import React from 'react';
import { SettingsView } from 'csdm/ui/settings/settings-view';
import { SettingsEntry } from 'csdm/ui/settings/settings-entry';

// The upstream pane assumes an embedded or external PostgreSQL service and
// offers destructive maintenance actions. The Tauri startup slice instead
// owns a distinct local SQLite library and cannot operate on PostgreSQL data.
export function DatabaseSettings() {
  return (
    <SettingsView>
      <SettingsEntry
        title="Local library"
        description="This Tauri port stores its new library in SQLite under the app data directory. Existing CS Demo Manager PostgreSQL data is untouched."
        interactiveComponent={null}
      />
      <SettingsEntry
        title="Migration status"
        description="Importing an existing CS Demo Manager database and switching database servers are still being ported."
        interactiveComponent={null}
      />
    </SettingsView>
  );
}
