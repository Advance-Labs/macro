/**
 * Runs before the app bundle: index.html loads this small entry first, so a
 * signed-in web load asks for its identity while the bundle still downloads.
 * Keep its imports tiny, so it loads well ahead of the bundle.
 */
import { SERVER_HOSTS } from '@core/constant/servers';
import { startBootRequest } from '@core/util/boot-requests';
import { hasLoginCookie } from '@core/util/cookies';

// Native apps fetch through the Tauri HTTP client, so they skip the head start.
if (!('__TAURI_INTERNALS__' in window) && hasLoginCookie()) {
  startBootRequest(
    `${SERVER_HOSTS['auth-service']}/user/legacy_user_permissions`
  );
}
