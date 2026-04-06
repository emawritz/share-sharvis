<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { tr } from '$lib/i18n';

	function getFriendlyMessage(status: number, translate: (key: string, params?: Record<string, string | number>) => string): string {
		if (status === 404) return translate('error.notFound');
		if (status === 403) return translate('error.forbidden');
		if (status === 500) return translate('error.server');
		return page.error?.message || translate('error.title');
	}

	function retry() {
		window.location.reload();
	}

	function goHome() {
		goto('/');
	}
</script>

<div class="error-page">
	<div class="error-card">
		<div class="error-icon">
			<svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--red)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
				<circle cx="12" cy="12" r="10" />
				<line x1="12" y1="8" x2="12" y2="12" />
				<line x1="12" y1="16" x2="12.01" y2="16" />
			</svg>
		</div>

		<h1 class="error-title">{$tr('error.title')}</h1>

		<p class="error-code">{$tr('error.code', { code: String(page.status) })}</p>

		<p class="error-message">{getFriendlyMessage(page.status, $tr)}</p>

		<div class="error-actions">
			<button class="btn btn-primary" onclick={retry}>
				{$tr('error.retry')}
			</button>
			<button class="btn btn-secondary" onclick={goHome}>
				{$tr('error.goHome')}
			</button>
		</div>
	</div>
</div>

<style>
	.error-page {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 100vh;
		background: var(--bg-0);
		padding: 24px;
	}

	.error-card {
		text-align: center;
		background: var(--bg-1);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		padding: 48px 40px;
		max-width: 440px;
		width: 100%;
	}

	.error-icon {
		margin-bottom: 24px;
		opacity: 0.9;
	}

	.error-title {
		font-size: 24px;
		font-weight: 600;
		color: var(--text-0);
		margin: 0 0 8px 0;
	}

	.error-code {
		font-size: 14px;
		color: var(--text-2);
		font-family: monospace;
		margin: 0 0 16px 0;
	}

	.error-message {
		font-size: 16px;
		color: var(--text-1);
		margin: 0 0 32px 0;
		line-height: 1.5;
	}

	.error-actions {
		display: flex;
		gap: 12px;
		justify-content: center;
	}

	.btn {
		padding: 10px 24px;
		border-radius: 8px;
		font-size: 0.9rem;
		font-weight: 500;
		cursor: pointer;
		border: none;
		transition: opacity 0.15s;
	}

	.btn:hover {
		opacity: 0.85;
	}

	.btn-primary {
		background: var(--red);
		color: #fff;
	}

	.btn-secondary {
		background: var(--bg-2);
		color: var(--text-0);
		border: 1px solid var(--border);
	}
</style>
