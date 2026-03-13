// Refresh-available indicator for living profile.
// Derives from ProfileMetadataInfo.next_refresh_available (already fetched in ProfilesView).
// This store just tracks whether the notification dot should show.

let refreshAvailable = $state(false);

export function isRefreshAvailable(): boolean {
  return refreshAvailable;
}

export function setRefreshAvailable(available: boolean): void {
  refreshAvailable = available;
}
