# Media theory and a testable steering experiment

The media-theory connection guides interface design. It does not validate the
attachment's mental-state mappings. This note records a proposed next experiment;
no steering controller, neurofeedback training, or human calibration was added to
the renderer benchmark.

## Form shapes what the reader can inspect

McLuhan's *Understanding Media* examines how technologies shape communication and
human behavior. Borsuk's *The Book*, in MIT Press's Essential Knowledge series,
examines the book through its material form, contents, conceptual identity, and
interface. Sousanis makes visual thinking part of his argument through comics.
These are useful connections; they do not by themselves establish a documented
line of direct influence. [McLuhan](https://mitpress.mit.edu/9780262631594/understanding-media/),
[Borsuk](https://mitpress.mit.edu/9780262535410/the-book/),
[Sousanis](https://spinweaveandcut.com/about/).

My design inference: make the representation's choices inspectable. Three spatial
coordinates are not inherently more truthful than text or a two-dimensional plot.
Sousanis's work on audio description and tactile diagrams also makes accessibility
part of this discussion, rather than equating thought with sight.
[Author's account](https://spinweaveandcut.com/accessible-unflattening-modes-visual-thinking/).

For NeuralCompose, preserve measured values, missing values, provenance, and the
meaning of each coordinate. A rendered signal is not a geometry of consciousness.
Model candidate embeddings are model representations, not decoded human intentions.

## The proposed axes need validation

The attachment supplies no validation for verbal thought → one direction, imagery
→ the other, a torus → focus, or a Lorenz-like shape → creativity. A calibration
prompt does not establish those associations. Thirty seconds and two instructed
tasks do not establish three independent, repeatable controls.

AF7 and AF8 are anterior frontal recording sites. Averaging them does not create a
midline electrode measurement. Name an eventual feature by its actual computation
and montage. Specify whether powers or waveforms are combined; those operations
are not interchangeable. [Muse recording study](https://pmc.ncbi.nlm.nih.gov/articles/PMC10590850/).

Three proposed controls have specific alternative explanations. These are risks to
test, not findings that the unrecorded steering signal is entirely artifact.

| Proposed control | Alternative explanation and interpretation limit |
| --- | --- |
| AF8–AF7 log alpha-power difference | Frontal alpha asymmetry commonly uses F3/F4 and F7/F8; AF7/AF8 is a different montage. Forehead muscle activity and eye movements can alter anterior spectral power. Eyebrow or gaze control would not validate a verbal-versus-spatial axis. [Conventional pairs](https://pmc.ncbi.nlm.nih.gov/articles/PMC3984363/), [ocular artifact experiment](https://doi.org/10.1016/S1388-2457(00)00541-1). |
| Frontal theta/beta ratio | Muscle electrical activity (EMG) from forehead and jaw muscles overlaps beta. Contraction can change the denominator and move the control without the claimed cognitive change. Even weak contractions can mimic scalp rhythms. [Goncharova et al.](https://pubmed.ncbi.nlm.nih.gov/12948787/). |
| Four-channel phase-locking value (PLV) | Channels measured as `s1-r` and `s2-r` share the reference term `r`. It can contribute apparent phase synchrony without coupling between the intended sources. The reference activity need not itself be non-neural. Volume conduction adds a separate common-source problem. [Reference analysis](https://pmc.ncbi.nlm.nih.gov/articles/PMC2891575/), [Nolte et al.](https://pubmed.ncbi.nlm.nih.gov/15351371/). |

Muse spectral/asymmetry validation has been studied with specified processing and
references; that does not establish the attachment's semantic controls.
[Cannard et al., author preprint](https://www.biorxiv.org/content/10.1101/2021.11.02.466989v1.full).
Reference effects and shared mains interference are both common-input concerns;
this review does not establish the cause of an earlier 60 Hz observation.

Use feature names such as log alpha-power difference or theta/beta ratio until
mental-state interpretations have separate evidence. Invalid powers, zero
variance, insufficient data, and unusable denominators produce unavailable values.
An epsilon must not silently convert undefined normalization into a valid center.

## Assistance must remain visible

Keep a measured trajectory and any assisted cursor as separate, labeled objects.
A gravity well can be a game mechanic. Its attraction must not count as evidence
that the participant generated the target signal. Likewise, a designed torus or
butterfly is a simulation unless the displayed samples actually produce it.

Retain neutral colors and descriptive labels. Targets can refer to measured
features without naming them “flow,” “deep focus,” or “creative incubation.” Freeze
normalization within a trial, or explicitly record an adaptive baseline. Otherwise,
a rolling baseline can change the control map while the participant learns it.

A candidate pilot would first test whether facial or eye movement explains control.
Include labeled eyebrow, jaw, blink, and gaze trials, with independent muscle and
eye-activity measurements where available. Predefine artifact exclusions and report
performance on held-out retained trials. Four EEG channels alone do not establish
that these sources have been separated. Then use one declared feature, repeated
randomized cue blocks, and a suitable feedback control condition.
Predefine the endpoint and failure rule before collecting data;
report failed calibration and nonresponse. These are proposed design choices, not
results or an approved human-study protocol. CRED-nf provides a relevant framework
for subsequent study design and reporting.
[Consensus paper](https://academic.oup.com/brain/article/143/6/1674/5807912).

## Frame timing does not measure the whole feedback loop

The implemented native path uses Rust and wgpu. EEG still enters the existing JSON
stream contract; semantic snapshots use bounded local JSON IPC. The benchmark does
not demonstrate that JSON must be bypassed or that Metal/OpenGL is required.

The attachment's update equation is an exponential moving average, not hysteresis:

`P[t] = (1-m) P[t-1] + m S[t]`

Its mean sample age, also its low-frequency group delay, is `(1-m)/m` update periods.
For m=0.05–0.15 this is approximately 94–317 ms at 60 Hz, or 22–74 ms at 256 Hz.
These values are derived from the stated equation. The update rate must be specified.
Hysteresis instead uses different transition thresholds depending on current state.

The current band estimate uses 512 samples at 256 Hz: two seconds of history. This
is window support, not a claim that its output has exactly two seconds of delay.
The largest recorded receipt-to-submit p95, 40.97 ms, starts inside the viewer.
It excludes electrode/amplifier acquisition, Bluetooth, bridge processing, and
display presentation. It also does not measure the feature estimator's response.
Sensor-to-screen latency remains unknown; this benchmark supplies no multiplier
for estimating it. Measure those stages separately for steering.

There is no basis here for a universal 10–20 ms embodiment cutoff. For example, a
rubber-hand experiment found stronger ownership effects below 300 ms of visual–tactile
discrepancy than at longer tested delays. That task-specific result is neither a BCI
latency target nor a universal replacement threshold.
[Shimada et al.](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0006185).
