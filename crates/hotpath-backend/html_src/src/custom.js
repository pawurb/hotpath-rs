(function () {
    const params = new URLSearchParams(window.location.search);
    const status = params.get("waitlist");
    if (!status) return;

    params.delete("waitlist");
    const qs = params.toString();
    history.replaceState({}, "", window.location.pathname + (qs ? "?" + qs : ""));

    if (status === "joined") {
        const card = document.querySelector(".waitlist-card");
        if (card) {
            card.innerHTML =
                '<h2 class="waitlist-card-title">🎉 You\'re in!</h2>' +
                '<p>We\'ll email you when Hotpath Team is ready for early access.</p>';
        }
        return;
    }

    const toast = document.createElement("div");
    toast.className = "waitlist-toast";
    toast.textContent = "Something went wrong - please try again.";
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 6000);
})();
