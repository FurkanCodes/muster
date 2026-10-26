# Sound kit

Real recordings from Mixkit, fetched into `kit/audio` by `fetch-audio.sh` (each file's own address is there) and measured with numpy by `measure.py`: decoded to mono at 22,050 Hz, a sound's peak is the loudest point of its loudness over 64 samples (2.9 ms), from the start of the file.

Mixkit's licenses allow commercial use; they do not allow passing the files on as they are, so they are fetched, not kept in git. Music: https://mixkit.co/license/#musicFree. Sound effects: https://mixkit.co/license/#sfxFree.

The coin for "paid" is left out: nothing in Muster is paid for. The rewind takes its place, for a turn put back.

| File | What it is | Mixkit page | License | Length | Peak |
|---|---|---|---|---|---|
| `music.mp3` | Rising Forest, Diego Nava (House), the music | https://mixkit.co/free-stock-music/house/ | Mixkit Stock Music Free License | 98.724 s | see the beat grid |
| `click.mp3` | Classic click: a pointer clicks | https://mixkit.co/free-sound-effects/click/ | Mixkit Sound Effects Free License | 288 ms | 135 ms |
| `typing.mp3` | Soft typing on a digital keyboard: a prompt is typed | https://mixkit.co/free-sound-effects/typing/ | Mixkit Sound Effects Free License | 26213 ms | 13303 ms |
| `whoosh.mp3` | Fast whoosh transition: a scene moves away | https://mixkit.co/free-sound-effects/whoosh/ | Mixkit Sound Effects Free License | 1757 ms | 721 ms |
| `pop.mp3` | Long pop: something new appears in the list | https://mixkit.co/free-sound-effects/pop/ | Mixkit Sound Effects Free License | 479 ms | 35 ms |
| `ding.mp3` | Bell notification: an agent needs you | https://mixkit.co/free-sound-effects/notification/ | Mixkit Sound Effects Free License | 3361 ms | 216 ms |
| `success.mp3` | Success software tone: an agent finishes | https://mixkit.co/free-sound-effects/success/ | Mixkit Sound Effects Free License | 1619 ms | 261 ms |
| `impact.mp3` | Movie impact intro presentation: the drop | https://mixkit.co/free-sound-effects/impact/ | Mixkit Sound Effects Free License | 5994 ms | 1270 ms |
| `rewind.mp3` | Fast tape rewind cinematic transition: a turn is put back | https://mixkit.co/free-sound-effects/rewind/ | Mixkit Sound Effects Free License | 1917 ms | 409 ms |

## The music

- **Tempo:** 123.97 BPM: a beat every 484.0 ms, a bar of four every 1936.0 ms.
- **First downbeat:** 1.626 s (beat 3; beats 0 to 2 are a pickup).
- **Drop:** 46.153 s, beat 95, a downbeat: the next four bars are 3.8× the four before in loudness and low end together. The bars before it, from about 31 s, are the quiet build.
- **Bars:** they start on beats 3, 7, 11, … (every fourth from the first downbeat). In house every beat has the same kick, so bars are found from the drop, which comes in on a downbeat, not from the kick.
- **Beat grid:** beat *n* is at 0.174 + *n* × 0.48399 s, 204 beats to 98.424 s. Every beat, in seconds:

```
  0:   0.174   0.658   1.142   1.626   2.110   2.594   3.078   3.562   4.046   4.530   5.014   5.498
 12:   5.982   6.466   6.950   7.434   7.918   8.402   8.886   9.370   9.854  10.338  10.822  11.306
 24:  11.790  12.274  12.758  13.242  13.726  14.210  14.694  15.178  15.662  16.146  16.630  17.114
 36:  17.598  18.082  18.566  19.050  19.534  20.018  20.502  20.986  21.470  21.954  22.438  22.922
 48:  23.406  23.890  24.374  24.858  25.342  25.826  26.310  26.793  27.277  27.761  28.245  28.729
 60:  29.213  29.697  30.181  30.665  31.149  31.633  32.117  32.601  33.085  33.569  34.053  34.537
 72:  35.021  35.505  35.989  36.473  36.957  37.441  37.925  38.409  38.893  39.377  39.861  40.345
 84:  40.829  41.313  41.797  42.281  42.765  43.249  43.733  44.217  44.701  45.185  45.669  46.153
 96:  46.637  47.121  47.605  48.089  48.573  49.057  49.541  50.025  50.509  50.993  51.477  51.961
108:  52.445  52.929  53.413  53.897  54.381  54.865  55.349  55.833  56.317  56.801  57.285  57.769
120:  58.253  58.737  59.221  59.705  60.189  60.673  61.157  61.641  62.125  62.609  63.093  63.577
132:  64.061  64.545  65.029  65.513  65.997  66.481  66.965  67.448  67.932  68.416  68.900  69.384
144:  69.868  70.352  70.836  71.320  71.804  72.288  72.772  73.256  73.740  74.224  74.708  75.192
156:  75.676  76.160  76.644  77.128  77.612  78.096  78.580  79.064  79.548  80.032  80.516  81.000
168:  81.484  81.968  82.452  82.936  83.420  83.904  84.388  84.872  85.356  85.840  86.324  86.808
180:  87.292  87.776  88.260  88.744  89.228  89.712  90.196  90.680  91.164  91.648  92.132  92.616
192:  93.100  93.584  94.068  94.552  95.036  95.520  96.004  96.488  96.972  97.456  97.940  98.424
```

`beats.png` draws the waveform with every beat (thin), every bar (blue), the first downbeat (green) and the drop (red), and a close-up on the drop with beats numbered.
