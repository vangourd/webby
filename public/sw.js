self.addEventListener('push', function (event) {
    const data = event.data ? event.data.json() : {};
    const title = data.title || 'Webby';
    const options = {
        body: data.body || '',
        icon: '/favicon.ico',
        data: data.data || {},
    };
    event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener('notificationclick', function (event) {
    event.notification.close();
    const url = event.notification.data && event.notification.data.url;
    if (url) {
        event.waitUntil(clients.openWindow(url));
    }
});
