<script lang="ts">
	import '$lib/styles/theme.css';
	import favicon from '$lib/assets/favicon.svg';
	import Toast from '$lib/components/Toast.svelte';
	import { addToast } from '$lib/stores/notifications';
	import { initVisibility } from '$lib/stores/visibility';
	import { tr } from '$lib/i18n';

	let { children } = $props();

	$effect(() => {
		return initVisibility();
	});

	$effect(() => {
		const onError = (e: ErrorEvent) => {
			console.error('Unhandled error:', e.error);
			addToast(`Error: ${e.message}`, 'error');
		};
		const onRejection = (e: PromiseRejectionEvent) => {
			console.error('Unhandled rejection:', e.reason);
			addToast(`Error: ${e.reason}`, 'error');
		};
		window.addEventListener('error', onError);
		window.addEventListener('unhandledrejection', onRejection);
		return () => {
			window.removeEventListener('error', onError);
			window.removeEventListener('unhandledrejection', onRejection);
		};
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<meta name="theme-color" content="#060a10" />
	<!-- Anti-flash: apply saved theme before first paint -->
	<!-- eslint-disable-next-line svelte/no-at-html-tags -->
	{@html `<script>(function(){var t=localStorage.getItem('jarvis-theme');if(t==='light'||t==='dark')document.documentElement.setAttribute('data-theme',t);}());<\/script>`}
</svelte:head>

<a href="#main-content" class="skip-link">{$tr('a11y.skipToContent')}</a>
{@render children()}
<Toast />
<div id="liveRegion" class="sr-only" aria-live="polite" role="status"></div>
