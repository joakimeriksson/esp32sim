# iPhone browser observations — September 20

Investigation deferred at the user’s request. No fix or diagnosed root cause. [Screenshot transcription](observations.json) records Pocket Tank on iPhone 17 Pro, reported iOS 27 release with Low Power Mode and Lockdown Mode off. Original screenshots remain local as IMG_3351.PNG (JIT on) and IMG_3352.PNG (JIT off); they are not published here.

JIT on: 2.4% average realtime at 61 seconds, 4,173 successfully installed modules and zero compilation failures. JIT off: 0.9% average at 103 seconds. Both displayed 100.0% execution and 0.0% output/gaps in the sampled window. Rounded host timings point toward execution rather than worker waiting; these are unmatched manual observations, not a controlled speedup comparison.

The 0.48-second compile counter measures synchronous module creation and installation, not browser background optimization. Successful installation does not identify WebKit’s native compilation tier. Interpreter-tier execution is an unconfirmed hypothesis. A tiny JavaScript/Wasm page-versus-worker probe was proposed but explicitly not pursued.

The temporary opt-in diagnostic source is retained in diagnostic.patch and its new modules below. The selected emulator binary was unchanged. Desktop checks verified counters, report export, JIT on/off and visibility above the fold at 393×700; they do not validate phone performance.

Research leads, not diagnoses: [WebKit execution tiers](https://webkit.org/blog/17899/introducing-the-jetstream-3-benchmark-suite/), [firsthand Safari 27 JavaScript slowdown report with a non-reproducing reply](https://www.reddit.com/r/Safari/comments/1w5ba7d/safari_27_beta_has_a_severe_javascriptcore/) and [older Unity startup slowdown reports](https://discussions.unity.com/t/frame-rate-drops-at-game-beginning-on-ios-safari/830304). None establishes the cause here.
