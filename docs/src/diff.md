# hotpath Diff

**Work in progress.** hotpath Diff is performance regression testing for Rust pull requests: it compares the report a pull request produced with the one its base branch produced, judges the difference under a per-repository policy and posts the verdict as a comment on the pull request. This page will document how to set it up; until then, the waitlist below is where to sign up for early access.

<div class="waitlist-card" id="waitlist">
  <h2 class="waitlist-card-title"><span class="waitlist-card-brand">hotpath Diff</span> - every Rust PR gets a performance review</h2>
  <p>Catch regressions in memory, SQL queries, HTTP calls and concurrency bottlenecks before they reach production. Iterate on reproducible signals, not CI noise.</p>
  <p><span class="waitlist-card-brand">hotpath Diff</span> will also provide agents with clear performance constraints and help ensure that AI-built applications stay fast.</p>
  <img src="{{#asset-hash images/hotpath-team-poc.webp}}" class="waitlist-card-image" alt="Hotpath Team commit timeline comparing duration, memory, HTTP and SQL metrics across commits, flagging a PR that introduced 171 new SQL calls" loading="lazy" width="1672" height="941">
  <p class="waitlist-cta-note">Launching soon • Early access invitations will be sent to waitlist members first.</p>
  <div class="waitlist-cta-row">
    <a href="/auth/github/login" class="waitlist-cta"><svg class="waitlist-cta-icon" viewBox="0 0 16 16" width="18" height="18" aria-hidden="true" fill="currentColor"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"></path></svg>Join waitlist with GitHub</a>
  </div>
</div>
<p class="waitlist-building-note">Building in public. Follow development progress on X: <a href="https://x.com/pawelurbanekcom" target="_blank" rel="noopener noreferrer">@pawelurbanekcom</a></p>
