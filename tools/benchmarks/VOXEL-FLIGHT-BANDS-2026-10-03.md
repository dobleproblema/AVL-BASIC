# Voxel Flight: bandas de color — 2026-10-03

**Informe histórico.** Las variantes `samples/g-voxel-flight.bas` y
`samples/g-voxel-flight-original.bas` ya no están en el checkout actual.
La muestra disponible es [`g-voxel.bas`](../../samples/g-voxel.bas), con otros
cambios; las métricas siguientes no describen su rendimiento ni su equivalencia.
Las rutas antiguas se conservan como identidad de las fuentes medidas. Para
repetir las pruebas hay que restaurarlas y cotejar los hashes de esta tanda.

El sombreado constante de cada tramo entre dos cortes de profundidad producía franjas visibles al mover la cámara.
Este refinamiento interpola el color con corrección de perspectiva y subdivide cada tramo según su variación RGB.
El divisor nominal es 3 niveles de canal; no constituye una cota global del error visual.
La caché recupera la muestra inmediatamente anterior aunque estuviera oculta; profundidad, proyección, niebla y razones de perspectiva se precalculan.
Se conservan el terreno filtrado, su iluminación, la trayectoria y el espaciado de profundidad de la versión anterior.

**Fuentes y alcance.** «Antes» en las pruebas significa la versión que ya corregía las agujas, no la original restaurada.

| Fuente | SHA-256 |
|---|---|
| Original restaurada, `samples/g-voxel-flight-original.bas`, intacta | `781b2528ff111e51edc52433bde1659985805dee629be404d89c6f2dd9e7f0f8` |
| Antes, `target/voxel-bands-20261003/before.bas` | `4fe4101afb99a26a6f03aa1656fd89d3777f53852866ba41845b0303c9de5305` |
| Después, `samples/g-voxel-flight.bas` | `74d665acf4912ea704da9f1c5a0c5ce60746282fe91a324b8719ecf71fb86495` |

Ejecutable Windows existente: `target/release/avl-basic.exe`, SHA-256 `c6ff712fad985071490ece9a1d4d151b857096475ae239c0aad7a4c566192da5`.
No se instaló ni compiló nada. Los arneses y las evidencias permanecen localmente bajo `target/`.

**Prueba aislada de bandas.** `target/voxel-bands-20261003/flat-water-final/report.json`: `passed`.
Se fuerza `HM=SEA-2` antes de iluminar, conservando las ondas espaciales de luz y los mapas RGB, idénticos entre variantes.
Cámara `(36.123,216.217,0,65,0)`, 320 columnas, semilla 42; referencia analítica del plano por centro de píxel y columna.
Se comparan 199.040 píxeles, con interpolación bilineal de RGB y niebla continua; esta prueba no modela relieve ni oclusión real.

| Medida, canales RGB de 0 a 255 | Antes | Después |
|---|---:|---:|
| RMS frente a referencia, fase Z=3 | 3,4786 | 1,5901 |
| RMS al cambiar sólo fase Z de 3 a 3,1 | 1,3568 | 0,6326 |
| Máximo cambio de canal entre fases | 24 | 6 |

El máximo error frente a la referencia sigue siendo 44; los resultados no demuestran eliminación total de las discontinuidades.

**Secuencia y controles.** `temporal-before/report.json` y `temporal-row/report.json`, bajo `target/voxel-bands-20261003/`, pasan.
Son 24 imágenes con `DT=1/30`, cámara inicial `(40,130,0.17,70,-10)` y desplazamiento `(0.015,0.12)` por imagen.
HM, trayectoria y cámara tienen hashes idénticos entre variantes; las líneas ejecutables de `gradient-row.bas` coincidían con la fuente medida de aquella tanda.
La media del RMS entre imágenes consecutivas pasa de 1,9963 a 1,7841; los píxeles con salto de canal >16, de 694,5 a 365,0 por pareja.
Son descriptores de esta trayectoria, no una puntuación universal de calidad: también incluyen movimiento legítimo y bordes geométricos.
`control-final/report.json` pasa sus tres ejecuciones de 84 imágenes: repeticiones idénticas y HUD sin alterar los estados.
Incluye Q/H, manual/auto, giro, velocidad, altura, envoltura de coordenadas/ángulos, transición suave y regeneración R sin sumar su latencia a T.

**Coste medido.** `target/voxel-bands-20261003/final-window/report.json`: `passed`, ventana real, sin límite de FPS ni HUD.
Semilla 42, `DT=1/30`, 60 imágenes medidas, cinco de calentamiento interno y un par externo descartado; cuatro pares AB/BA por resolución.
El tiempo excluye inicialización y exportaciones; HM, cámara y PNG son deterministas dentro de cada variante. Mediana de FPS, con MAD entre paréntesis:

| Columnas | Antes | Después |
|---:|---:|---:|
| 160 | 99,14 (0,68) | 60,21 (0,05) |
| 320 | 58,26 (0,11) | 33,46 (0,06) |
| 640 | 31,69 (0,05) | 17,86 (0,02) |

El sombreado nuevo tiene un coste real: aproximadamente 39–44 % menos FPS en este equipo y recorrido.
El programa normal mantiene `FRAME 60`. Estos resultados no predicen el rendimiento de otro equipo.
Permanecen cortes de profundidad, píxeles y cambios de visibilidad; puede quedar parpadeo geométrico o residual de color.

**Reproducción local.** Ejecutar desde la raíz del repositorio Rust, con las fuentes históricas y los arneses locales restaurados, sin otros intérpretes ni mediciones simultáneas.
Cada salida debe ser una carpeta nueva; los nombres siguientes no sobrescriben las evidencias citadas.

```powershell
py -3 target/voxel-bands-20261003/flat_water_probe.py --baseline target/voxel-bands-20261003/before.bas --candidate samples/g-voxel-flight.bas --output target/voxel-bands-20261003/repro-flat-water
py -3 target/voxel-bands-20261003/temporal_probe.py --source target/voxel-bands-20261003/before.bas --output target/voxel-bands-20261003/repro-temporal-before --camera 40 130 .17 70 -10
py -3 target/voxel-bands-20261003/temporal_probe.py --source samples/g-voxel-flight.bas --output target/voxel-bands-20261003/repro-temporal-after --camera 40 130 .17 70 -10
py -3 target/voxel-flight-20261003/check_controls.py --source samples/g-voxel-flight.bas --output target/voxel-bands-20261003/repro-controls
py -3 target/voxel-flight-20261003/probe.py --source target/voxel-bands-20261003/before.bas --candidate samples/g-voxel-flight.bas --output target/voxel-bands-20261003/repro-window --cols 160,320,640 --frames 60 --runs 4 --warmups 1 --internal-warmup 5 --window 1
```

Las capturas temporales se exportan sin interpretar su duración como rendimiento; sólo el último comando constituye una comparación de FPS.
