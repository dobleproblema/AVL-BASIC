# Voxel Flight: estabilidad del terreno — 3 de octubre de 2026

**Informe histórico.** Las variantes `samples/g-voxel-flight.bas` y
`samples/g-voxel-flight-original.bas` ya no están en el checkout actual.
La muestra disponible es [`g-voxel.bas`](../../samples/g-voxel.bas), con otros
cambios; las cifras y conclusiones siguientes describen exclusivamente las
fuentes medidas e identificadas por sus hashes. Para repetir los comandos se
necesitan esas fuentes exactas y las evidencias locales citadas.

La variante medida `samples/g-voxel-flight.bas` sustituía el vecino más cercano por
altura y color continuos, reduciendo los saltos y destellos de las orillas.
**Tiene coste:** a 320 columnas pasa de 95,31 a 57,93 FPS locales sin límite.
No se ha medido en el Surface ni con batería.

## Cambio aplicado

- Una pasada periódica del filtro separable `[1,2,1]/4` por eje atenúa detalles
  de una celda, conservando la semilla y las formas generales del terreno.
- Altura bilineal antes de `MAX(SEA,H)`; la cámara usa ese mismo terreno.
  Los rayos se centran en cada columna.
- RGB iluminado por vértice sin truncar altura ni luz. Coeficientes precalculados
  `base + dx*x + dy*y + dxy*x*y` reconstruyen bilineal exactamente; sólo las
  franjas visibles interpolan color. Se elimina la tabla de 524.288 colores.
- Niebla continua y ondas acuáticas periódicas también en los bordes del mundo.
- Autopiloto con transiciones suaves de rumbo, velocidad e inclinación, giro
  por el arco más corto y ángulo acotado. El reloj excluye la regeneración.
  Se conservan los controles y el límite normal de 60 FPS.

## Rendimiento con ventana real

Evidencia: `target/voxel-flight-20261003/final-window/report.json`.
Windows 11, mismo ejecutable, semilla 42, HUD/entrada desactivados, `DT=1/30`,
sin límite. Por resolución: un par de calentamiento descartado y cuatro pares
medidos en orden AB, BA, AB, BA. Cada ejecución calienta cinco fotogramas y mide
60; se excluyen inicialización, calentamiento y exportación de terreno/PNG/cámara.
Valores **mediana ± MAD**, que no representan intervalos de confianza:

| Columnas | Antes, FPS | Después, FPS | Antes, ms/fotograma | Después, ms/fotograma |
|---:|---:|---:|---:|---:|
| 160 | 154,83 ± 0,36 | 98,65 ± 0,33 | 6,459 ± 0,015 | 10,137 ± 0,034 |
| 320 | 95,31 ± 0,38 | 57,93 ± 0,07 | 10,492 ± 0,041 | 17,261 ± 0,019 |
| 640 | 54,35 ± 0,17 | 31,73 ± 0,12 | 18,401 ± 0,056 | 31,520 ± 0,117 |

El autopiloto cambia ligeramente la trayectoria: **no es la misma cámara entre
versiones**, ni una medición aislada del renderizador. Terreno, cámara y PNG son
deterministas dentro de cada variante. A 320 columnas, la inicialización baja
de 0,48757 ± 0,00056 s a 0,17483 ± 0,00165 s; no se incluye en las FPS.

## Comprobaciones y límites

`surface-validation.json`: filtro centrado correcto con error máximo `2,84e-14`,
altura media conservada y cambio RMS de 0,541 unidades. En 8.962 fronteras
próximas al agua, el salto máximo anterior era 5,589; las muestras nuevas a
ambos lados, separadas `2e-6`, difieren como máximo `7,76e-6`. Error periódico
`1,28e-13`. Es continuidad espacial, no una puntuación de calidad de vídeo.

`coast-probes/report.json`: a 320 columnas y desplazamiento de cámara de 0,02,
los píxeles con variación superior a 32 en algún canal bajan de **216 a 78**
en costa, 60 a 32 en montaña y 104 a 50 cerca del borde. RMS por canal costero:
1,823 → 0,956. Son tres pruebas concretas; la interpolación cambia más píxeles
en pequeñas cantidades. El prototipo `compact` probado tiene exactamente el
mismo renderizador/generador final; la cámara fija evita cambios del autopiloto.

`control-check/report.json`: dos repeticiones deterministas y una con HUD pasan
85 pasos/84 fotogramas. Verifican Q/H/espacio, control manual, límites, separación
del suelo, envoltura de posición/ángulo, retorno suave al autopiloto, R y reloj
sin tiempo de regeneración. El HUD conserva el estado de simulación.

Permanece aliasing lejano por los pasos crecientes en profundidad y la
cuantización a píxeles; no hay filtrado dependiente de distancia. Las mediciones
son cortas y locales, sin garantía de 60 FPS en todos los paisajes o equipos.

## Evidencia y reproducción local

SHA-256 cotejados durante la medición con el informe y las fuentes de aquella tanda:

| Archivo | SHA-256 |
|---|---|
| `target/voxel-flight-20261003/before.bas` | `781b2528ff111e51edc52433bde1659985805dee629be404d89c6f2dd9e7f0f8` |
| `samples/g-voxel-flight.bas` | `4fe4101afb99a26a6f03aa1656fd89d3777f53852866ba41845b0303c9de5305` |
| `target/release/avl-basic.exe` | `c6ff712fad985071490ece9a1d4d151b857096475ae239c0aad7a4c566192da5` |

La versión anterior, scripts y resultados se conservan localmente en
`target/voxel-flight-20261003/`; no se distribuyen ni aparecen en un clon nuevo.
Desde la raíz, con esa carpeta conservada, la fuente histórica restaurada y
sin otros benchmarks simultáneos:

```powershell
py -3 target/voxel-flight-20261003/probe.py --source target/voxel-flight-20261003/before.bas --candidate samples/g-voxel-flight.bas --output target/voxel-flight-20261003/repeat-window --cols 160,320,640 --frames 60 --runs 4 --warmups 1 --internal-warmup 5 --window 1
```

Para una vista costera fija, usar otra salida y sustituir opciones por
`--cols 320 --frames 1 --runs 1 --warmups 0 --internal-warmup 0 --window 0
--camera 36.123 216.217 0 65 0`; repetir con X=36.143 en otra salida.
El informe de cada ejecución guarda hashes y rutas de los PNG; el script local
`image_metrics.py` acepta dos PNG y calcula las diferencias.
