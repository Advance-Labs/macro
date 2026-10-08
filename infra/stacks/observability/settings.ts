export interface Settings {
  region: string;
  grafanaHost: string;
  otlpHost: string;
  allowedEmails: string[];
  adminEmails: string[];
  secretArn: string;
  volumeId: string;
  logsBucket: string;
  tracesBucket: string;
}

// Restrict values before interpolating into INI, nginx, shell and JMESPath.
export function validateSettings(settings: Settings): void {
  for (const host of [settings.grafanaHost, settings.otlpHost]) {
    if (!/^[a-z0-9-]+\.macro\.com$/.test(host)) {
      throw new Error(
        'Observability hostnames must be subdomains of macro.com'
      );
    }
  }
  if (settings.grafanaHost === settings.otlpHost) {
    throw new Error('UI and ingestion require separate hostnames');
  }
  if (!settings.allowedEmails.length || !settings.adminEmails.length) {
    throw new Error('Configure approved users and at least one admin');
  }
  for (const email of [...settings.allowedEmails, ...settings.adminEmails]) {
    if (!/^[a-z0-9._+-]+@macro\.com$/.test(email)) {
      throw new Error(
        'Access lists require lowercase macro.com email addresses'
      );
    }
  }
  if (
    settings.adminEmails.some(
      (email) => !settings.allowedEmails.includes(email)
    )
  ) {
    throw new Error('Every admin must also be in allowedEmails');
  }
  if (
    !/^arn:aws:secretsmanager:[a-z0-9-]+:\d{12}:secret:[a-zA-Z0-9/_+=.@-]+$/.test(
      settings.secretArn
    )
  ) {
    throw new Error(
      'secretArn must reference an existing Secrets Manager secret'
    );
  }
  if (!/^[a-z]{2}-[a-z]+-\d$/.test(settings.region)) {
    throw new Error('Invalid AWS region');
  }
  if (!/^vol-[a-f0-9]+$/.test(settings.volumeId)) {
    throw new Error('Invalid data volume ID');
  }
  for (const bucket of [settings.logsBucket, settings.tracesBucket]) {
    if (!/^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$/.test(bucket)) {
      throw new Error('Invalid telemetry bucket name');
    }
  }
}

export function roleExpression(settings: Settings): string {
  const list = (emails: string[]) => `\`${JSON.stringify(emails)}\``;
  return `contains(${list(settings.adminEmails)}, email) && 'GrafanaAdmin' || contains(${list(settings.allowedEmails)}, email) && 'Viewer' || 'Denied'`;
}
