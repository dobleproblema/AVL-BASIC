10     REM ================================================================
20     REM AVL CRYSTAL SIEGE
30     REM Rescue the golden core; use the remaining orbs for demolition.
40     REM Arrows orbit, W/S zoom, click/Space fires. D demonstrates a siege.
50     REM R rebuilds, T slows time, P pauses, Q changes solver accuracy.
60     REM F toggles the frame cap, V sparks, B music, N mute, M mode, Esc exits.
70     REM ================================================================

75     ScreenMode=640 : REM Display width: 640, 800 or 1024; M cycles in game.

80     ON ERROR GOTO RuntimeError
90     CALL InitializeGame

100    WHILE QuitRequested=0
110        CurrentTime=TIME : FrameDelta=MAX(0.0001,MIN(0.05,CurrentTime-PreviousFrameTime)) : PreviousFrameTime=CurrentTime : AnimationTime=AnimationTime+FrameDelta
120        CALL ReadControls
130        IF QuitRequested THEN EXIT WHILE
140        CALL RunFrame(FrameDelta)
150    WEND

160    CALL CloseGame
170    END

180    RuntimeError:
190        FailureCode=ERR : FailureLine=ERL
200        ON ERROR RESUME NEXT
210        AUDIO STOP : MOUSE 1 : SCREEN CLOSE
220        PRINT "Crystal Siege error ";FailureCode;" at line ";FailureLine
230        ON ERROR GOTO 0
240        END

250    DEF SUB InitializeGame()
260        SCREEN : RAD : MAT BASE 0 : CALL ConfigureDisplay : CLG
270        DIM NodeScreenX(219),NodeScreenY(219),NodeInverseDepth(219),PaletteRed(6),PaletteGreen(6),PaletteBlue(6)
280        DIM AutoTargetNode(5),AutoLaunchX(5),AutoLaunchZ(5),AutoCameraYaw(5),AutoCameraPitch(5),AutoCameraDistance(5)
290        DIM StarX(89),StarY(89),StarColor(89)
300        DIM SparkX(179),SparkY(179),SparkZ(179),SparkVelocityX(179),SparkVelocityY(179),SparkVelocityZ(179),SparkLifetime(179),SparkSize(179),SparkColor(179)
310        SMALLFONT TRANSPARENT : INK RGB(145,230,243) : LOCATE ROUND(WIDTH/16-26),ROUND(14*ScreenScaleY)
320        GPRINT "FAST STACKING - GROWING A CATHEDRAL..." : FRAME

330        CALL LoadPaletteAndSky
340        CALL ResetStructure : CALL ResetPanels : CALL ResetBeams : CALL ResetRubble
350        CALL LoadAudio
360        LimitFrameRate=1 : SolverIterations=3 : SlowMotion=0 : Paused=0 : MusicEnabled=1 : Muted=0 : ShowSparks=1
370        CameraYaw=-1.04 : CameraPitch=0.28 : CameraDistance=22 : CameraTargetY=4.4 : FocalLength=535 : ViewCenterY=232
380        AimScreenX=320 : AimScreenY=265 : PreviousMouseX=MOUSEX : PreviousMouseY=MOUSEY : FireHeld=0
390        SimulationTime=0 : AnimationTime=0 : PhysicsAccumulator=0 : FramesPerSecond=0 : FpsFrameCount=0 : FrameNumber=0 : LastFpsTime=TIME : PreviousFrameTime=TIME
400        NextSpark=0 : BestRescueShots=0 : RescueAwarded=0 : RescueBannerUntil=-1 : RescueShots=0
410        DestructionScore=0 : BestDestructionScore=0 : PeakFallenFraction=0 : PeakHeightLoss=0 : QuitRequested=0
420        CALL UpdateMusic
430    SUBEND

431    DEF SUB ConfigureDisplay()
432        IF ScreenMode<>640 AND ScreenMode<>800 AND ScreenMode<>1024 THEN ScreenMode=640
433        MODE ScreenMode : SCALE 0,639,0,479 : PAPER RGB(5,7,17) : SMALLFONT TRANSPARENT
434        DIM DepthBuffer(WIDTH-1,HEIGHT-1) : MAT DepthBuffer=0
435        ScreenScaleX=(WIDTH-1)/639 : ScreenScaleY=(HEIGHT-1)/479
436        PreviousMouseX=MOUSEX : PreviousMouseY=MOUSEY
437    SUBEND

440    DEF SUB CloseGame()
450        AUDIO STOP : MOUSE 1 : SCREEN CLOSE
460        ON ERROR GOTO 0
470    SUBEND

471    DEF SUB CycleDisplay()
472        WHILE KEYDOWN(77) : FRAME : WEND
473        ScreenMode=IIF(ScreenMode=640,800,IIF(ScreenMode=800,1024,640))
474        CALL ConfigureDisplay : PreviousFrameTime=TIME
475    SUBEND

480    DEF SUB RebuildGame()
490        CALL ResetStructure : CALL ResetPanels : CALL ResetBeams : CALL ResetRubble
500        MAT SparkLifetime=0 : PhysicsAccumulator=0 : SimulationTime=0 : Paused=0 : AutoSiege=0 : NextSpark=0
510        RescueAwarded=0 : RescueBannerUntil=-1 : RescueShots=0 : RescueEvent=0
520        DestructionScore=0 : PeakFallenFraction=0 : PeakHeightLoss=0
530        CALL RestartAudio : PreviousFrameTime=TIME
540    SUBEND

550    DEF SUB ReadControls()
560        LOCAL cameraInput, fireDown, projected(2)
570        FrameKey$=UPPER$(INKEY$)
580        IF FrameKey$=CHR$(27) OR KEYDOWN(27) THEN QuitRequested=1 : EXIT SUB

590        IF FrameKey$="R" THEN
600            CALL RebuildGame
610        ELSEIF FrameKey$="D" THEN
620            CALL RebuildGame
630            AutoSiege=1 : AutoStage=0 : NextAutoShotTime=1 : SlowMotion=0
640        ELSEIF FrameKey$="T" THEN
650            SlowMotion=1-SlowMotion
660        ELSEIF FrameKey$="P" THEN
670            Paused=1-Paused
680        ELSEIF FrameKey$="F" THEN
690            LimitFrameRate=1-LimitFrameRate
700        ELSEIF FrameKey$="Q" THEN
710            SolverIterations=SolverIterations+1 : IF SolverIterations>5 THEN SolverIterations=2
720        ELSEIF FrameKey$="B" THEN
730            MusicEnabled=1-MusicEnabled : CALL UpdateMusic
740        ELSEIF FrameKey$="N" THEN
750            Muted=1-Muted : CALL UpdateMute
752        ELSEIF FrameKey$="M" THEN
754            CALL CycleDisplay
760        ENDIF

770        cameraInput=ABS(KEYDOWN(28))+ABS(KEYDOWN(29))+ABS(KEYDOWN(30))+ABS(KEYDOWN(31))+ABS(KEYDOWN(87))+ABS(KEYDOWN(83))
780        IF cameraInput>0 THEN AutoSiege=0
790        IF AutoSiege THEN CALL UpdateDemoCamera
800        CameraYaw=CameraYaw+(ABS(KEYDOWN(29))-ABS(KEYDOWN(28)))*FrameDelta*0.85
810        CameraPitch=FNClamp(CameraPitch+(ABS(KEYDOWN(30))-ABS(KEYDOWN(31)))*FrameDelta*0.45,0.09,0.72)
820        CameraDistance=FNClamp(CameraDistance+(ABS(KEYDOWN(83))-ABS(KEYDOWN(87)))*FrameDelta*8,16,32)
830        CALL UpdateCamera

840        IF AutoSiege AND AutoStage<6 THEN
850            CALL ProjectPoint(NodeX(AutoTargetNode(AutoStage)),NodeY(AutoTargetNode(AutoStage)),NodeZ(AutoTargetNode(AutoStage)),projected)
860            IF projected(2)>0 THEN AimScreenX=FNClamp(projected(0),12,628) : AimScreenY=FNClamp(projected(1),55,427)
870        ENDIF
880        IF MOUSEX<>PreviousMouseX OR MOUSEY<>PreviousMouseY THEN
890            IF MOUSEX>8 AND MOUSEX<632 AND MOUSEY>48 AND MOUSEY<431 THEN AimScreenX=MOUSEX : AimScreenY=MOUSEY
900        ENDIF
910        PreviousMouseX=MOUSEX : PreviousMouseY=MOUSEY
920        fireDown=ABS(MOUSELEFT) OR ABS(KEYDOWN(32))
930        IF fireDown AND FireHeld=0 AND Paused=0 THEN AutoSiege=0 : CALL FireFromCrosshair
940        FireHeld=fireDown
950        IF AutoSiege AND Paused=0 THEN CALL UpdateAutoSiege
960    SUBEND

970    DEF SUB RunFrame(frameSeconds)
980        LOCAL physicsBegin, drawBegin
990        physicsBegin=TIME
1000       CALL AdvanceSimulation(frameSeconds)
1010       IF Paused=0 THEN CALL StepEffects(frameSeconds,TimeScale)
1020       CALL UpdateAudio
1030       PhysicsMilliseconds=PhysicsMilliseconds*0.85+(TIME-physicsBegin)*150

1040       drawBegin=TIME : CALL RenderWorld
1050       DrawingMilliseconds=DrawingMilliseconds*0.85+(TIME-drawBegin)*150
1060       CALL DrawHud
1070       IF LimitFrameRate THEN FRAME 60 ELSE FRAME
1080       FrameNumber=FrameNumber+1 : FpsFrameCount=FpsFrameCount+1
1090       IF TIME-LastFpsTime>=0.5 THEN FramesPerSecond=FpsFrameCount/(TIME-LastFpsTime) : FpsFrameCount=0 : LastFpsTime=TIME
1100   SUBEND

1110   DEF SUB AdvanceSimulation(frameSeconds)
1120       LOCAL steps
1130       PhysicsStep=1/90 : TimeScale=IIF(SlowMotion,0.22,1)
1140       IF Paused=0 THEN PhysicsAccumulator=PhysicsAccumulator+frameSeconds*TimeScale
1150       steps=0
1160       WHILE Paused=0 AND PhysicsAccumulator>=PhysicsStep AND steps<4
1170           CALL StepStructure : CALL UpdateDestructionScore
1180           CALL StepPanels : CALL StepBeams : CALL StepRubble
1190           SimulationTime=SimulationTime+PhysicsStep : PhysicsAccumulator=PhysicsAccumulator-PhysicsStep : steps=steps+1
1200       WEND
1210       IF steps>=4 THEN PhysicsAccumulator=MIN(PhysicsAccumulator,PhysicsStep)
1220   SUBEND

1230   DEF SUB UpdateCamera()
1240       LOCAL cosYaw, sinYaw, cosPitch, sinPitch
1250       cosYaw=COS(CameraYaw) : sinYaw=SIN(CameraYaw) : cosPitch=COS(CameraPitch) : sinPitch=SIN(CameraPitch)
1260       CameraForwardX=-cosYaw*cosPitch : CameraForwardY=-sinPitch : CameraForwardZ=-sinYaw*cosPitch
1270       CameraRightX=sinYaw : CameraRightY=0 : CameraRightZ=-cosYaw
1280       CameraUpX=-cosYaw*sinPitch : CameraUpY=cosPitch : CameraUpZ=-sinYaw*sinPitch
1290       CameraX=-CameraForwardX*CameraDistance : CameraY=CameraTargetY-CameraForwardY*CameraDistance : CameraZ=-CameraForwardZ*CameraDistance
1300   SUBEND

1310   REM Projected is an output array: screen X, screen Y and inverse depth.
1320   DEF SUB ProjectPoint(worldX,worldY,worldZ,projected)
1330       LOCAL deltaX, deltaY, deltaZ, depth
1340       deltaX=worldX-CameraX : deltaY=worldY-CameraY : deltaZ=worldZ-CameraZ
1350       depth=deltaX*CameraForwardX+deltaY*CameraForwardY+deltaZ*CameraForwardZ
1360       projected(0)=0 : projected(1)=0 : projected(2)=0
1370       IF depth<0.35 THEN EXIT SUB
1380       projected(2)=1/depth
1390       projected(0)=320+FocalLength*(deltaX*CameraRightX+deltaZ*CameraRightZ)*projected(2)
1400       projected(1)=ViewCenterY+FocalLength*(deltaX*CameraUpX+deltaY*CameraUpY+deltaZ*CameraUpZ)*projected(2)
1410   SUBEND

1420   DEF SUB FireFromCrosshair()
1430       LOCAL horizontal, vertical, rayLength
1440       horizontal=(AimScreenX-320)/FocalLength : vertical=(AimScreenY-ViewCenterY)/FocalLength
1450       AimDirectionX=CameraForwardX+CameraRightX*horizontal+CameraUpX*vertical
1460       AimDirectionY=CameraForwardY+CameraRightY*horizontal+CameraUpY*vertical
1470       AimDirectionZ=CameraForwardZ+CameraRightZ*horizontal+CameraUpZ*vertical
1480       rayLength=SQR(AimDirectionX*AimDirectionX+AimDirectionY*AimDirectionY+AimDirectionZ*AimDirectionZ)
1490       AimDirectionX=AimDirectionX/rayLength : AimDirectionY=AimDirectionY/rayLength : AimDirectionZ=AimDirectionZ/rayLength
1500       LaunchX=CameraX+AimDirectionX*0.9 : LaunchY=CameraY+AimDirectionY*0.9 : LaunchZ=CameraZ+AimDirectionZ*0.9
1510       CALL LaunchOrb
1520   SUBEND

1530   DEF SUB UpdateAutoSiege()
1540       LOCAL targetNode, flightTime
1550       IF CathedralDemolished THEN AutoSiege=0 : EXIT SUB
1560       IF SimulationTime<NextAutoShotTime OR ProjectileActive THEN EXIT SUB
1570       LaunchX=AutoLaunchX(AutoStage) : LaunchY=7 : LaunchZ=AutoLaunchZ(AutoStage)
1580       targetNode=AutoTargetNode(AutoStage)
1590       AimDirectionX=NodeX(targetNode)-LaunchX : AimDirectionY=NodeY(targetNode)-LaunchY : AimDirectionZ=NodeZ(targetNode)-LaunchZ
1600       flightTime=SQR(AimDirectionX*AimDirectionX+AimDirectionY*AimDirectionY+AimDirectionZ*AimDirectionZ)/32
1610       AimDirectionY=AimDirectionY+0.6*flightTime*flightTime
1620       CALL LaunchOrb
1630       AutoStage=AutoStage+1 : NextAutoShotTime=SimulationTime+3
1640       IF AutoStage>=6 THEN AutoSiege=0
1650   SUBEND

1660   DEF SUB UpdateDemoCamera()
1670       LOCAL yawDelta, ease
1680       IF AutoStage>=6 THEN EXIT SUB
1690       yawDelta=AutoCameraYaw(AutoStage)-CameraYaw
1700       yawDelta=yawDelta-2*PI*INT((yawDelta+PI)/(2*PI))
1710       ease=MIN(1,FrameDelta*2.8)
1720       CameraYaw=CameraYaw+yawDelta*ease
1730       CameraPitch=CameraPitch+(AutoCameraPitch(AutoStage)-CameraPitch)*ease
1740       CameraDistance=CameraDistance+(AutoCameraDistance(AutoStage)-CameraDistance)*ease
1750   SUBEND

1760   DEF FNClamp(value,lowerBound,upperBound)=MAX(lowerBound,MIN(upperBound,value))

1770   DEF FNDestructionPoints(brokenLinks,linkCount,fallenPeak,heightPeak)
1780       FNDestructionPoints=MIN(10000,INT(3000*FNClamp(brokenLinks/MAX(1,linkCount),0,1)+3000*fallenPeak+4000*heightPeak))
1790   FNEND

1800   DEF SUB UpdateRescueRecord()
1810       LOCAL usedShots
1820       usedShots=MaxShots-ShotsLeft
1830       IF usedShots<1 THEN EXIT SUB
1840       IF BestRescueShots=0 OR usedShots<BestRescueShots THEN BestRescueShots=usedShots
1850   SUBEND

1860   DEF SUB UpdateDestructionScore()
1870       IF ImpactCount=0 AND BrokenLinkCount=0 THEN EXIT SUB
1880       PeakFallenFraction=MAX(PeakFallenFraction,MIN(1,MAX(0,FallenFraction)))
1890       PeakHeightLoss=MAX(PeakHeightLoss,MIN(1,MAX(0,HeightLossFraction)))
1900       DestructionScore=MAX(DestructionScore,FNDestructionPoints(BrokenLinkCount,LinkCount,PeakFallenFraction,PeakHeightLoss))
1910       BestDestructionScore=MAX(BestDestructionScore,DestructionScore)
1920   SUBEND

1930   DEF SUB LoadAudio()
1940       AUDIO ON
1950       AUDIO LOAD 1,"assets/audio/crystal-siege/silent-prism-loop.wav"
1960       AUDIO LOAD 2,"assets/audio/crystal-siege/launch.wav"
1970       AUDIO LOAD 3,"assets/audio/crystal-siege/impact-glass.wav"
1980       AUDIO LOAD 4,"assets/audio/crystal-siege/fracture.wav"
1990       AUDIO LOAD 5,"assets/audio/crystal-siege/victory.wav"
2000       NextFractureSoundTime=0 : NextImpactSoundTime=0
2010   SUBEND

2020   DEF SUB UpdateAudio()
2030       IF LaunchEvent AND Muted=0 THEN AUDIO PLAY 2,2,0,0.60
2040       IF ImpactEvents AND AnimationTime>=NextImpactSoundTime THEN
2050           NextImpactSoundTime=AnimationTime+0.10
2060           IF Muted=0 THEN AUDIO PLAY 3,3,0,0.60
2070       ENDIF
2080       IF FractureEvents>0 AND AnimationTime>=NextFractureSoundTime THEN
2090           NextFractureSoundTime=AnimationTime+0.12
2100           IF Muted=0 THEN AUDIO PLAY 4,4,0,0.55,0,0.9+(BrokenLinkCount MOD 5)*0.05
2110       ENDIF
2120       IF ImpactEvents OR FractureEvents>0 THEN CALL SpawnSparks
2130       RescueEvent=0
2140       IF CoreReachedFloor AND RescueAwarded=0 THEN
2150           RescueAwarded=1 : RescueBannerUntil=AnimationTime+3 : RescueShots=MaxShots-ShotsLeft : RescueEvent=1
2160       ENDIF
2170       IF RescueEvent THEN CALL UpdateRescueRecord
2180       IF RescueEvent AND Muted=0 THEN AUDIO PLAY 5,5,0,0.70
2190       LaunchEvent=0 : ImpactEvents=0 : FractureEvents=0 : DemolitionEvent=0 : RescueEvent=0
2200   SUBEND

2210   DEF SUB UpdateMusic()
2220       IF MusicEnabled=0 OR Muted THEN AUDIO STOP 1 : EXIT SUB
2230       AUDIO PLAY 1,1,1,0.65
2240   SUBEND

2250   DEF SUB RestartAudio()
2260       AUDIO STOP : NextFractureSoundTime=AnimationTime : CALL UpdateMusic
2270   SUBEND

2280   DEF SUB UpdateMute()
2290       IF Muted THEN AUDIO OFF : EXIT SUB
2300       AUDIO ON : CALL UpdateMusic
2310   SUBEND

2320   DEF SUB StepEffects(frameSeconds,timeScale)
2330       LOCAL index, effectDelta
2340       effectDelta=frameSeconds*timeScale
2350       FOR index=0 TO 179
2360           IF SparkLifetime(index)<=0 THEN GOTO ContinueSpark
2370           SparkLifetime(index)=SparkLifetime(index)-effectDelta : SparkVelocityY(index)=SparkVelocityY(index)-effectDelta*6
2380           SparkX(index)=SparkX(index)+SparkVelocityX(index)*effectDelta
2390           SparkY(index)=SparkY(index)+SparkVelocityY(index)*effectDelta
2400           SparkZ(index)=SparkZ(index)+SparkVelocityZ(index)*effectDelta
2410           IF SparkY(index)<0.15 THEN
2420               SparkY(index)=0.15 : SparkVelocityY(index)=ABS(SparkVelocityY(index))*0.25
2430               SparkVelocityX(index)=SparkVelocityX(index)*0.7 : SparkVelocityZ(index)=SparkVelocityZ(index)*0.7
2440           ENDIF
2450       ContinueSpark:
2460       NEXT index
2470   SUBEND

2480   DEF SUB SpawnSparks()
2490       LOCAL count, index, newSpark, angle
2500       IF ShowSparks=0 THEN EXIT SUB
2510       count=MIN(8,2+FractureEvents+ImpactEvents*2)
2520       FOR newSpark=1 TO count
2530           index=NextSpark : NextSpark=(NextSpark+1) MOD 180
2540           angle=index*2.39996+AnimationTime*1.7
2550           SparkX(index)=EventX : SparkY(index)=EventY : SparkZ(index)=EventZ
2560           SparkVelocityX(index)=COS(angle)*(1.2+(index MOD 5)*0.45)
2570           SparkVelocityZ(index)=SIN(angle)*(1.2+(index MOD 7)*0.3)
2580           SparkVelocityY(index)=1+(index MOD 9)*0.45
2590           SparkLifetime(index)=0.7+(index MOD 11)*0.075
2600           SparkSize(index)=0.028+(index MOD 4)*0.012 : SparkColor(index)=1+(index MOD 3)
2610       NEXT newSpark
2620   SUBEND

2630   DEF SUB LoadPaletteAndSky()
2640       LOCAL index, randomValue, luminance
2650       PaletteRed(0)=93 : PaletteGreen(0)=211 : PaletteBlue(0)=220 : PaletteRed(1)=70 : PaletteGreen(1)=215 : PaletteBlue(1)=245
2660       PaletteRed(2)=174 : PaletteGreen(2)=122 : PaletteBlue(2)=249 : PaletteRed(3)=246 : PaletteGreen(3)=105 : PaletteBlue(3)=196
2670       PaletteRed(4)=255 : PaletteGreen(4)=216 : PaletteBlue(4)=100 : PaletteRed(5)=80 : PaletteGreen(5)=255 : PaletteBlue(5)=145
2680       PaletteRed(6)=180 : PaletteGreen(6)=205 : PaletteBlue(6)=244
2690       FOR index=0 TO 89
2700           randomValue=SIN(index*127.1+31.7)*43758.5453
2710           StarX(index)=INT((randomValue-INT(randomValue))*640)
2720           randomValue=SIN(index*269.5+13.3)*43758.5453
2730           StarY(index)=45+INT((randomValue-INT(randomValue))*390)
2740           luminance=40+(index MOD 7)*9
2750           StarColor(index)=RGB(luminance*0.75,luminance*0.86,luminance)
2760       NEXT index
2770       RESTORE 93000
2780       FOR index=0 TO 5
2790           READ AutoTargetNode(index),AutoLaunchX(index),AutoLaunchZ(index),AutoCameraYaw(index),AutoCameraPitch(index),AutoCameraDistance(index)
2800       NEXT index
2810   SUBEND
2820   REM ===============================================================
2830   REM Cathedral structure and continuous projectile collisions
2840   REM Persistent world arrays are shared; temporary calculations are local.

2850   DEF FNNodeDistance(PositionsX, PositionsY, PositionsZ, FirstNode, SecondNode)
2860       LOCAL DeltaX, DeltaY, DeltaZ
2870       DeltaX=PositionsX(FirstNode)-PositionsX(SecondNode)
2880       DeltaY=PositionsY(FirstNode)-PositionsY(SecondNode)
2890       DeltaZ=PositionsZ(FirstNode)-PositionsZ(SecondNode)
2900       FNNodeDistance=SQR(DeltaX^2+DeltaY^2+DeltaZ^2)
2910   FNEND

2920   DEF SUB ResetStructure()
2930       LOCAL NodeIndex, EdgeIndex, FaceIndex, FirstNode, SecondNode

2940       IF SceneInitialized=0 THEN
2950           DIM NodeX(219),NodeY(219),NodeZ(219),PreviousNodeX(219),PreviousNodeY(219),PreviousNodeZ(219)
2960           DIM RestNodeX(219),RestNodeY(219),RestNodeZ(219),InverseMass(219),NodeKind(219),NodeRadius(219),NodeColor(219)
2970           DIM NodeDamage(219),WeakBearing(219),BearingHit(219),NodeHasLanded(219)
2980           DIM LinkNodeA(499),LinkNodeB(499),LinkRestLength(499),LinkActive(499),LinkStrain(499),LinkHeat(499)
2990           DIM PanelNodeA(95),PanelNodeB(95),PanelNodeC(95),PanelLink1(95),PanelLink2(95),PanelLink3(95)

3000           NodeCount=77 : LinkCount=266 : CoreNode=76
3010           RESTORE 90000
3020           FOR NodeIndex=0 TO NodeCount-1
3030               READ RestNodeX(NodeIndex),RestNodeY(NodeIndex),RestNodeZ(NodeIndex),InverseMass(NodeIndex),NodeKind(NodeIndex),NodeRadius(NodeIndex),NodeColor(NodeIndex)
3040           NEXT NodeIndex

3050           RESTORE 91000
3060           FOR EdgeIndex=0 TO LinkCount-1
3070               READ LinkNodeA(EdgeIndex),LinkNodeB(EdgeIndex)
3080               FirstNode=LinkNodeA(EdgeIndex) : SecondNode=LinkNodeB(EdgeIndex)
3090               LinkRestLength(EdgeIndex)=FNNodeDistance(RestNodeX,RestNodeY,RestNodeZ,FirstNode,SecondNode)
3100           NEXT EdgeIndex

3110           PanelCount=64 : RESTORE 92000
3120           FOR FaceIndex=0 TO PanelCount-1
3130               READ PanelNodeA(FaceIndex),PanelNodeB(FaceIndex),PanelNodeC(FaceIndex),PanelLink1(FaceIndex),PanelLink2(FaceIndex),PanelLink3(FaceIndex)
3140           NEXT FaceIndex
3150           MAT WeakBearing=0 : WeakBearing(1)=1 : WeakBearing(11)=1 : WeakBearing(27)=1 : WeakBearing(37)=1 : WeakBearing(53)=1 : WeakBearing(63)=1
3160           SceneInitialized=1
3170       ENDIF

3180       FOR NodeIndex=0 TO NodeCount-1
3190           NodeX(NodeIndex)=RestNodeX(NodeIndex) : NodeY(NodeIndex)=RestNodeY(NodeIndex) : NodeZ(NodeIndex)=RestNodeZ(NodeIndex)
3200           PreviousNodeX(NodeIndex)=NodeX(NodeIndex) : PreviousNodeY(NodeIndex)=NodeY(NodeIndex) : PreviousNodeZ(NodeIndex)=NodeZ(NodeIndex)
3210           NodeDamage(NodeIndex)=0 : BearingHit(NodeIndex)=0 : NodeHasLanded(NodeIndex)=0
3220       NEXT NodeIndex
3230       MAT LinkActive=1 : MAT LinkStrain=0 : MAT LinkHeat=0
3240       MaxShots=10 : ShotsLeft=MaxShots : BrokenLinkCount=0 : ImpactCount=0 : CathedralDemolished=0
3250       ProjectileActive=0 : ProjectileLifetime=0 : StructureTime=0
3260       LaunchEvent=0 : ImpactEvents=0 : FractureEvents=0 : DemolitionEvent=0 : BreakingForImpact=0
3270       FallenNodeCount=0 : FallenFraction=0 : CoreReachedFloor=0 : BearingHits=0 : BearingsBroken=0 : BearingCount=6 : HeightLossFraction=0
3280       IF SolverIterations<1 THEN SolverIterations=3
3290   SUBEND

3300   DEF SUB StepStructure()
3310       LOCAL NodeIndex, EdgeIndex, Iteration, GravityStep
3320       LOCAL VelocityX, VelocityY, VelocityZ, FirstNode, SecondNode, InverseMassSum
3330       LOCAL DeltaX, DeltaY, DeltaZ, Distance, Stretch, FailureLimit, Correction
3340       LOCAL MobileCount, TotalDrop, TotalRestHeight

3350       StructureTime=StructureTime+PhysicsStep
3360       IF ProjectileActive THEN CALL StepProjectile
3370       GravityStep=9.0*PhysicsStep*PhysicsStep

3380       FOR NodeIndex=0 TO NodeCount-1
3390           IF InverseMass(NodeIndex)>0 THEN
3400               VelocityX=(NodeX(NodeIndex)-PreviousNodeX(NodeIndex))*0.994
3410               VelocityY=(NodeY(NodeIndex)-PreviousNodeY(NodeIndex))*0.994
3420               VelocityZ=(NodeZ(NodeIndex)-PreviousNodeZ(NodeIndex))*0.994
3430               PreviousNodeX(NodeIndex)=NodeX(NodeIndex) : PreviousNodeY(NodeIndex)=NodeY(NodeIndex) : PreviousNodeZ(NodeIndex)=NodeZ(NodeIndex)
3440               NodeX(NodeIndex)=NodeX(NodeIndex)+VelocityX : NodeY(NodeIndex)=NodeY(NodeIndex)+VelocityY-GravityStep : NodeZ(NodeIndex)=NodeZ(NodeIndex)+VelocityZ
3450               NodeDamage(NodeIndex)=MAX(0,NodeDamage(NodeIndex)-PhysicsStep*0.04)
3460           ENDIF
3470       NEXT NodeIndex

3480       FOR Iteration=1 TO SolverIterations
3490           FOR EdgeIndex=0 TO LinkCount-1
3500               IF LinkActive(EdgeIndex)=0 THEN GOTO NextConstraint
3510               FirstNode=LinkNodeA(EdgeIndex) : SecondNode=LinkNodeB(EdgeIndex)
3520               InverseMassSum=InverseMass(FirstNode)+InverseMass(SecondNode)
3530               IF InverseMassSum=0 THEN GOTO NextConstraint

3540               DeltaX=NodeX(SecondNode)-NodeX(FirstNode) : DeltaY=NodeY(SecondNode)-NodeY(FirstNode) : DeltaZ=NodeZ(SecondNode)-NodeZ(FirstNode)
3550               Distance=SQR(DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ)
3560               IF Distance<0.00001 THEN GOTO NextConstraint
3570               Stretch=ABS(Distance-LinkRestLength(EdgeIndex))/LinkRestLength(EdgeIndex)
3580               LinkStrain(EdgeIndex)=MAX(Stretch,LinkStrain(EdgeIndex)*0.93)
3590               FailureLimit=0.045*(1-0.7*MAX(NodeDamage(FirstNode),NodeDamage(SecondNode)))
3600               IF FirstNode=CoreNode OR SecondNode=CoreNode THEN FailureLimit=FailureLimit*0.65

3610               IF Stretch>FailureLimit AND StructureTime>0.35 THEN
3620                   CALL BreakLink(EdgeIndex)
3630               ELSE
3640                   Correction=(Distance-LinkRestLength(EdgeIndex))/Distance/InverseMassSum*0.88
3650                   NodeX(FirstNode)=NodeX(FirstNode)+DeltaX*Correction*InverseMass(FirstNode)
3660                   NodeY(FirstNode)=NodeY(FirstNode)+DeltaY*Correction*InverseMass(FirstNode)
3670                   NodeZ(FirstNode)=NodeZ(FirstNode)+DeltaZ*Correction*InverseMass(FirstNode)
3680                   NodeX(SecondNode)=NodeX(SecondNode)-DeltaX*Correction*InverseMass(SecondNode)
3690                   NodeY(SecondNode)=NodeY(SecondNode)-DeltaY*Correction*InverseMass(SecondNode)
3700                   NodeZ(SecondNode)=NodeZ(SecondNode)-DeltaZ*Correction*InverseMass(SecondNode)
3710               ENDIF
3720           NextConstraint:
3730           NEXT EdgeIndex

3740           FOR NodeIndex=0 TO NodeCount-1
3750               IF InverseMass(NodeIndex)=0 OR NodeY(NodeIndex)>=0.25+NodeRadius(NodeIndex) THEN GOTO NextGroundNode
3760               VelocityY=NodeY(NodeIndex)-PreviousNodeY(NodeIndex)
3770               REM Capture broken links before the ground projection changes their poses.
3780               IF NodeHasLanded(NodeIndex)=0 AND VelocityY<-3.0*PhysicsStep THEN CALL FractureGroundNode(NodeIndex)
3790               NodeY(NodeIndex)=0.25+NodeRadius(NodeIndex)
3800               PreviousNodeY(NodeIndex)=NodeY(NodeIndex)+MIN(0,VelocityY)*0.12
3810               PreviousNodeX(NodeIndex)=NodeX(NodeIndex)-(NodeX(NodeIndex)-PreviousNodeX(NodeIndex))*0.72
3820               PreviousNodeZ(NodeIndex)=NodeZ(NodeIndex)-(NodeZ(NodeIndex)-PreviousNodeZ(NodeIndex))*0.72
3830           NextGroundNode:
3840           NEXT NodeIndex
3850       NEXT Iteration

3860       FallenNodeCount=0 : MobileCount=0 : TotalDrop=0 : TotalRestHeight=0
3870       FOR NodeIndex=0 TO NodeCount-1
3880           IF InverseMass(NodeIndex)>0 THEN
3890               MobileCount=MobileCount+1
3900               TotalDrop=TotalDrop+MAX(0,RestNodeY(NodeIndex)-NodeY(NodeIndex))
3910               TotalRestHeight=TotalRestHeight+RestNodeY(NodeIndex)-0.25-NodeRadius(NodeIndex)
3920               IF RestNodeY(NodeIndex)-NodeY(NodeIndex)>=1.5 THEN FallenNodeCount=FallenNodeCount+1
3930           ENDIF
3940       NEXT NodeIndex
3950       FallenFraction=FallenNodeCount/MAX(1,MobileCount) : HeightLossFraction=TotalDrop/MAX(1,TotalRestHeight) : MobileNodeCount=MobileCount
3960       IF NodeY(CoreNode)<1.05 THEN CoreReachedFloor=1
3970       IF CathedralDemolished=0 AND FallenFraction>=0.68 AND HeightLossFraction>=0.58 THEN CathedralDemolished=1 : DemolitionEvent=1
3980   SUBEND

3990   DEF SUB StepProjectile()
4000       LOCAL StartX, StartY, StartZ, SweepX, SweepY, SweepZ, SweepLengthSquared
4010       LOCAL EdgeIndex, FirstNode, SecondNode, SideX, SideY, SideZ, SideLengthSquared
4020       LOCAL OffsetX, OffsetY, OffsetZ, SweepDotSide, SweepDotOffset, SideDotOffset
4030       LOCAL Denominator, ShotFraction, EdgeFraction, DeltaX, DeltaY, DeltaZ, BestHit(1)

4040       StartX=ProjectileX : StartY=ProjectileY : StartZ=ProjectileZ
4050       ProjectileVelocityY=ProjectileVelocityY-1.2*PhysicsStep
4060       ProjectileX=ProjectileX+ProjectileVelocityX*PhysicsStep : ProjectileY=ProjectileY+ProjectileVelocityY*PhysicsStep : ProjectileZ=ProjectileZ+ProjectileVelocityZ*PhysicsStep : ProjectileLifetime=ProjectileLifetime-PhysicsStep
4070       IF ProjectileLifetime<=0 OR ABS(ProjectileX)>35 OR ABS(ProjectileZ)>35 THEN ProjectileActive=0 : EXIT SUB

4080       SweepX=ProjectileX-StartX : SweepY=ProjectileY-StartY : SweepZ=ProjectileZ-StartZ
4090       SweepLengthSquared=SweepX*SweepX+SweepY*SweepY+SweepZ*SweepZ
4100       BestHit(0)=-1 : BestHit(1)=2
4110       FOR EdgeIndex=0 TO LinkCount-1
4120           IF LinkActive(EdgeIndex)=0 THEN GOTO NextProjectileEdge
4130           FirstNode=LinkNodeA(EdgeIndex) : SecondNode=LinkNodeB(EdgeIndex)
4140           IF ProjectileX<MIN(NodeX(FirstNode),NodeX(SecondNode))-0.9 OR ProjectileX>MAX(NodeX(FirstNode),NodeX(SecondNode))+0.9 THEN GOTO NextProjectileEdge
4150           IF ProjectileY<MIN(NodeY(FirstNode),NodeY(SecondNode))-0.9 OR ProjectileY>MAX(NodeY(FirstNode),NodeY(SecondNode))+0.9 THEN GOTO NextProjectileEdge
4160           IF ProjectileZ<MIN(NodeZ(FirstNode),NodeZ(SecondNode))-0.9 OR ProjectileZ>MAX(NodeZ(FirstNode),NodeZ(SecondNode))+0.9 THEN GOTO NextProjectileEdge

4170           SideX=NodeX(SecondNode)-NodeX(FirstNode) : SideY=NodeY(SecondNode)-NodeY(FirstNode) : SideZ=NodeZ(SecondNode)-NodeZ(FirstNode)
4180           SideLengthSquared=SideX*SideX+SideY*SideY+SideZ*SideZ
4190           IF SideLengthSquared<0.00001 THEN GOTO NextProjectileEdge
4200           OffsetX=StartX-NodeX(FirstNode) : OffsetY=StartY-NodeY(FirstNode) : OffsetZ=StartZ-NodeZ(FirstNode)
4210           SweepDotSide=SweepX*SideX+SweepY*SideY+SweepZ*SideZ
4220           SweepDotOffset=SweepX*OffsetX+SweepY*OffsetY+SweepZ*OffsetZ
4230           SideDotOffset=SideX*OffsetX+SideY*OffsetY+SideZ*OffsetZ
4240           Denominator=SweepLengthSquared*SideLengthSquared-SweepDotSide*SweepDotSide
4250           ShotFraction=0
4260           IF Denominator>1E-10 THEN ShotFraction=MAX(0,MIN(1,(SweepDotSide*SideDotOffset-SideLengthSquared*SweepDotOffset)/Denominator))
4270           EdgeFraction=MAX(0,MIN(1,(SweepDotSide*ShotFraction+SideDotOffset)/SideLengthSquared))
4280           ShotFraction=MAX(0,MIN(1,(SweepDotSide*EdgeFraction-SweepDotOffset)/MAX(1E-10,SweepLengthSquared)))
4290           EdgeFraction=MAX(0,MIN(1,(SweepDotSide*ShotFraction+SideDotOffset)/SideLengthSquared))
4300           DeltaX=OffsetX+SweepX*ShotFraction-SideX*EdgeFraction
4310           DeltaY=OffsetY+SweepY*ShotFraction-SideY*EdgeFraction
4320           DeltaZ=OffsetZ+SweepZ*ShotFraction-SideZ*EdgeFraction
4330           IF DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ>0.28 THEN GOTO NextProjectileEdge
4340           IF ShotFraction<BestHit(1) THEN BestHit(0)=EdgeIndex : BestHit(1)=ShotFraction
4350       NextProjectileEdge:
4360       NEXT EdgeIndex

4370       REM The hit array is an explicit two-value result, passed by reference.
4380       CALL TestPanelSweep(StartX,StartY,StartZ,SweepX,SweepY,SweepZ,BestHit)
4390       IF BestHit(0)>=0 THEN
4400           EventX=StartX+SweepX*BestHit(1) : EventY=StartY+SweepY*BestHit(1) : EventZ=StartZ+SweepZ*BestHit(1)
4410           CALL ImpactStructure(BestHit(0),EventX,EventY,EventZ)
4420           EXIT SUB
4430       ENDIF
4440       IF ProjectileY<0.45 THEN EventX=ProjectileX : EventY=0.3 : EventZ=ProjectileZ : ImpactEvents=ImpactEvents+1 : ProjectileActive=0
4450   SUBEND

4460   DEF SUB TestPanelSweep(StartX, StartY, StartZ, SweepX, SweepY, SweepZ, BestHit)
4470       LOCAL FaceIndex, FirstNode, SecondNode, ThirdNode
4480       LOCAL SideAX, SideAY, SideAZ, SideBX, SideBY, SideBZ
4490       LOCAL NormalX, NormalY, NormalZ, NormalLengthSquared, StartDistance, EndDistance, Radius
4500       LOCAL ShotFraction, PointX, PointY, PointZ, PlaneDistance
4510       LOCAL DotAA, DotBB, DotAB, DotPA, DotPB, Determinant, BarycentricU, BarycentricV

4520       FOR FaceIndex=0 TO PanelCount-1
4530           IF LinkActive(PanelLink1(FaceIndex))=0 OR LinkActive(PanelLink2(FaceIndex))=0 OR LinkActive(PanelLink3(FaceIndex))=0 THEN GOTO NextProjectilePanel
4540           FirstNode=PanelNodeA(FaceIndex) : SecondNode=PanelNodeB(FaceIndex) : ThirdNode=PanelNodeC(FaceIndex)
4550           IF ProjectileX<MIN(NodeX(FirstNode),MIN(NodeX(SecondNode),NodeX(ThirdNode)))-0.9 OR ProjectileX>MAX(NodeX(FirstNode),MAX(NodeX(SecondNode),NodeX(ThirdNode)))+0.9 THEN GOTO NextProjectilePanel
4560           IF ProjectileY<MIN(NodeY(FirstNode),MIN(NodeY(SecondNode),NodeY(ThirdNode)))-0.9 OR ProjectileY>MAX(NodeY(FirstNode),MAX(NodeY(SecondNode),NodeY(ThirdNode)))+0.9 THEN GOTO NextProjectilePanel
4570           IF ProjectileZ<MIN(NodeZ(FirstNode),MIN(NodeZ(SecondNode),NodeZ(ThirdNode)))-0.9 OR ProjectileZ>MAX(NodeZ(FirstNode),MAX(NodeZ(SecondNode),NodeZ(ThirdNode)))+0.9 THEN GOTO NextProjectilePanel

4580           SideAX=NodeX(SecondNode)-NodeX(FirstNode) : SideAY=NodeY(SecondNode)-NodeY(FirstNode) : SideAZ=NodeZ(SecondNode)-NodeZ(FirstNode)
4590           SideBX=NodeX(ThirdNode)-NodeX(FirstNode) : SideBY=NodeY(ThirdNode)-NodeY(FirstNode) : SideBZ=NodeZ(ThirdNode)-NodeZ(FirstNode)
4600           NormalX=SideAY*SideBZ-SideAZ*SideBY
4610           NormalY=SideAZ*SideBX-SideAX*SideBZ
4620           NormalZ=SideAX*SideBY-SideAY*SideBX
4630           NormalLengthSquared=NormalX*NormalX+NormalY*NormalY+NormalZ*NormalZ
4640           IF NormalLengthSquared<1E-9 THEN GOTO NextProjectilePanel
4650           StartDistance=(StartX-NodeX(FirstNode))*NormalX+(StartY-NodeY(FirstNode))*NormalY+(StartZ-NodeZ(FirstNode))*NormalZ
4660           EndDistance=(ProjectileX-NodeX(FirstNode))*NormalX+(ProjectileY-NodeY(FirstNode))*NormalY+(ProjectileZ-NodeZ(FirstNode))*NormalZ
4670           Radius=0.42*SQR(NormalLengthSquared)
4680           IF (StartDistance>Radius AND EndDistance>Radius) OR (StartDistance<-Radius AND EndDistance<-Radius) THEN GOTO NextProjectilePanel
4690           ShotFraction=0
4700           IF ABS(StartDistance)>Radius AND ABS(StartDistance-EndDistance)>1E-10 THEN ShotFraction=MAX(0,MIN(1,(StartDistance-SGN(StartDistance)*Radius)/(StartDistance-EndDistance)))
4710           PointX=StartX+SweepX*ShotFraction-NodeX(FirstNode)
4720           PointY=StartY+SweepY*ShotFraction-NodeY(FirstNode)
4730           PointZ=StartZ+SweepZ*ShotFraction-NodeZ(FirstNode)
4740           PlaneDistance=(PointX*NormalX+PointY*NormalY+PointZ*NormalZ)/NormalLengthSquared
4750           PointX=PointX-PlaneDistance*NormalX : PointY=PointY-PlaneDistance*NormalY : PointZ=PointZ-PlaneDistance*NormalZ
4760           DotAA=SideAX*SideAX+SideAY*SideAY+SideAZ*SideAZ
4770           DotBB=SideBX*SideBX+SideBY*SideBY+SideBZ*SideBZ
4780           DotAB=SideAX*SideBX+SideAY*SideBY+SideAZ*SideBZ
4790           DotPA=PointX*SideAX+PointY*SideAY+PointZ*SideAZ
4800           DotPB=PointX*SideBX+PointY*SideBY+PointZ*SideBZ
4810           Determinant=DotAA*DotBB-DotAB*DotAB
4820           IF Determinant<1E-10 THEN GOTO NextProjectilePanel
4830           BarycentricU=(DotPA*DotBB-DotPB*DotAB)/Determinant : BarycentricV=(DotPB*DotAA-DotPA*DotAB)/Determinant
4840           IF BarycentricU<-0.02 OR BarycentricV<-0.02 OR BarycentricU+BarycentricV>1.02 THEN GOTO NextProjectilePanel
4850           IF ShotFraction<BestHit(1) THEN BestHit(0)=PanelLink1(FaceIndex) : BestHit(1)=ShotFraction
4860       NextProjectilePanel:
4870       NEXT FaceIndex
4880   SUBEND

4890   DEF SUB ImpactStructure(HitEdge, ImpactX, ImpactY, ImpactZ)
4900       LOCAL LaunchSpeed, NearestBearing, NearestDistanceSquared, NodeIndex, EdgeIndex
4910       LOCAL DeltaX, DeltaY, DeltaZ, Distance, Impulse, FirstNode, SecondNode
4920       LOCAL LengthSquared, EdgeFraction, NearX, NearY, NearZ

4930       ProjectileActive=0 : ImpactCount=ImpactCount+1 : ImpactEvents=ImpactEvents+1
4940       LaunchSpeed=SQR(ProjectileVelocityX*ProjectileVelocityX+ProjectileVelocityY*ProjectileVelocityY+ProjectileVelocityZ*ProjectileVelocityZ) : LaunchSpeed=MAX(1,LaunchSpeed)
4950       NearestBearing=-1 : NearestDistanceSquared=0.75*0.75
4960       FOR NodeIndex=0 TO NodeCount-1
4970           IF WeakBearing(NodeIndex) AND BearingHit(NodeIndex)=0 THEN
4980               DeltaX=NodeX(NodeIndex)-ImpactX : DeltaY=NodeY(NodeIndex)-ImpactY : DeltaZ=NodeZ(NodeIndex)-ImpactZ
4990               Distance=DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ
5000               IF Distance<NearestDistanceSquared THEN NearestBearing=NodeIndex : NearestDistanceSquared=Distance
5010           ENDIF
5020       NEXT NodeIndex

5030       FOR NodeIndex=0 TO NodeCount-1
5040           IF InverseMass(NodeIndex)=0 THEN GOTO NextDamagedNode
5050           DeltaX=NodeX(NodeIndex)-ImpactX : DeltaY=NodeY(NodeIndex)-ImpactY : DeltaZ=NodeZ(NodeIndex)-ImpactZ
5060           Distance=SQR(DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ)
5070           IF Distance>=3 THEN GOTO NextDamagedNode
5080           NodeDamage(NodeIndex)=MIN(0.7,NodeDamage(NodeIndex)+0.6*(1-Distance/3))
5090           IF Distance>=2.8 THEN GOTO NextDamagedNode
5100           Impulse=(1-Distance/2.8)*20*PhysicsStep : Distance=MAX(0.35,Distance)
5110           PreviousNodeX(NodeIndex)=PreviousNodeX(NodeIndex)-Impulse*(ProjectileVelocityX/LaunchSpeed+DeltaX/Distance*0.35)
5120           PreviousNodeY(NodeIndex)=PreviousNodeY(NodeIndex)-Impulse*(ProjectileVelocityY/LaunchSpeed+DeltaY/Distance*0.35+0.12)
5130           PreviousNodeZ(NodeIndex)=PreviousNodeZ(NodeIndex)-Impulse*(ProjectileVelocityZ/LaunchSpeed+DeltaZ/Distance*0.35)
5140       NextDamagedNode:
5150       NEXT NodeIndex

5160       BreakingForImpact=1
5170       FOR EdgeIndex=0 TO LinkCount-1
5180           IF LinkActive(EdgeIndex) THEN
5190               FirstNode=LinkNodeA(EdgeIndex) : SecondNode=LinkNodeB(EdgeIndex)
5200               DeltaX=NodeX(SecondNode)-NodeX(FirstNode) : DeltaY=NodeY(SecondNode)-NodeY(FirstNode) : DeltaZ=NodeZ(SecondNode)-NodeZ(FirstNode)
5210               LengthSquared=DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ
5220               EdgeFraction=MAX(0,MIN(1,((ImpactX-NodeX(FirstNode))*DeltaX+(ImpactY-NodeY(FirstNode))*DeltaY+(ImpactZ-NodeZ(FirstNode))*DeltaZ)/MAX(0.00001,LengthSquared)))
5230               NearX=NodeX(FirstNode)+EdgeFraction*DeltaX-ImpactX
5240               NearY=NodeY(FirstNode)+EdgeFraction*DeltaY-ImpactY
5250               NearZ=NodeZ(FirstNode)+EdgeFraction*DeltaZ-ImpactZ
5260               IF EdgeIndex=HitEdge OR NearX*NearX+NearY*NearY+NearZ*NearZ<0.81 OR FirstNode=NearestBearing OR SecondNode=NearestBearing THEN CALL BreakLink(EdgeIndex)
5270           ENDIF
5280       NEXT EdgeIndex
5290       IF NearestBearing>=0 THEN BearingHit(NearestBearing)=1 : BearingHits=BearingHits+1 : BearingsBroken=BearingHits
5300       BreakingForImpact=0
5310   SUBEND

5320   DEF SUB BreakLink(EdgeIndex)
5330       REM The released beam inherits the intact edge's pose and momentum.
5340       IF BeamDebrisInitialized THEN CALL DetachBeam(EdgeIndex)
5350       LinkActive(EdgeIndex)=0 : LinkStrain(EdgeIndex)=1 : BrokenLinkCount=BrokenLinkCount+1 : FractureEvents=FractureEvents+1
5360       IF BreakingForImpact=0 THEN
5370           EventX=(NodeX(LinkNodeA(EdgeIndex))+NodeX(LinkNodeB(EdgeIndex)))*0.5
5380           EventY=(NodeY(LinkNodeA(EdgeIndex))+NodeY(LinkNodeB(EdgeIndex)))*0.5
5390           EventZ=(NodeZ(LinkNodeA(EdgeIndex))+NodeZ(LinkNodeB(EdgeIndex)))*0.5
5400       ENDIF
5410   SUBEND

5420   DEF SUB FractureGroundNode(NodeIndex)
5430       LOCAL EdgeIndex
5440       NodeHasLanded(NodeIndex)=1
5450       FOR EdgeIndex=0 TO LinkCount-1
5460           IF LinkActive(EdgeIndex) AND (LinkNodeA(EdgeIndex)=NodeIndex OR LinkNodeB(EdgeIndex)=NodeIndex) THEN CALL BreakLink(EdgeIndex)
5470       NEXT EdgeIndex
5480   SUBEND

5490   DEF SUB LaunchOrb()
5500       LOCAL AimLength
5510       IF ShotsLeft<=0 OR ProjectileActive THEN EXIT SUB
5520       AimLength=SQR(AimDirectionX*AimDirectionX+AimDirectionY*AimDirectionY+AimDirectionZ*AimDirectionZ)
5530       IF AimLength<0.00001 THEN EXIT SUB
5540       ProjectileX=LaunchX : ProjectileY=LaunchY : ProjectileZ=LaunchZ
5550       ProjectileVelocityX=AimDirectionX/AimLength*32 : ProjectileVelocityY=AimDirectionY/AimLength*32 : ProjectileVelocityZ=AimDirectionZ/AimLength*32
5560       ProjectileActive=1 : ProjectileLifetime=3.0 : ShotsLeft=ShotsLeft-1 : LaunchEvent=1
5570   SUBEND
5580   REM ===============================================================
5590   REM Persistent triangular panel debris

5600   DEF SUB ResetPanels()
5610       IF PanelDebrisInitialized=0 THEN
5620           DIM PanelX(95,2),PanelY(95,2),PanelZ(95,2),PreviousPanelX(95,2),PreviousPanelY(95,2),PreviousPanelZ(95,2)
5630           DIM PanelEdgeLength(95,2),PanelState(95),PanelQuietTicks(95)
5640           PanelDebrisInitialized=1
5650       ENDIF
5660       MAT PanelState=0 : MAT PanelQuietTicks=0
5670   SUBEND

5680   DEF SUB StepPanels()
5690       LOCAL FaceIndex, VertexIndex, PassIndex, NextVertex, VelocityX, VelocityY, VelocityZ
5700       LOCAL DeltaX, DeltaY, DeltaZ, Distance, Correction, Resting

5710       FOR FaceIndex=0 TO PanelCount-1
5720           IF PanelState(FaceIndex)=0 THEN
5730               IF LinkActive(PanelLink1(FaceIndex))=0 OR LinkActive(PanelLink2(FaceIndex))=0 OR LinkActive(PanelLink3(FaceIndex))=0 THEN CALL DetachPanel(FaceIndex)
5740           ENDIF
5750           IF PanelState(FaceIndex)<>1 THEN GOTO NextPanel

5760           FOR VertexIndex=0 TO 2
5770               VelocityX=(PanelX(FaceIndex,VertexIndex)-PreviousPanelX(FaceIndex,VertexIndex))*0.992
5780               VelocityY=(PanelY(FaceIndex,VertexIndex)-PreviousPanelY(FaceIndex,VertexIndex))*0.992
5790               VelocityZ=(PanelZ(FaceIndex,VertexIndex)-PreviousPanelZ(FaceIndex,VertexIndex))*0.992
5800               PreviousPanelX(FaceIndex,VertexIndex)=PanelX(FaceIndex,VertexIndex)
5810               PreviousPanelY(FaceIndex,VertexIndex)=PanelY(FaceIndex,VertexIndex)
5820               PreviousPanelZ(FaceIndex,VertexIndex)=PanelZ(FaceIndex,VertexIndex)
5830               PanelX(FaceIndex,VertexIndex)=PanelX(FaceIndex,VertexIndex)+VelocityX
5840               PanelY(FaceIndex,VertexIndex)=PanelY(FaceIndex,VertexIndex)+VelocityY-9*PhysicsStep*PhysicsStep
5850               PanelZ(FaceIndex,VertexIndex)=PanelZ(FaceIndex,VertexIndex)+VelocityZ
5860           NEXT VertexIndex

5870           FOR PassIndex=1 TO 3
5880               FOR VertexIndex=0 TO 2
5890                   NextVertex=(VertexIndex+1) MOD 3
5900                   DeltaX=PanelX(FaceIndex,NextVertex)-PanelX(FaceIndex,VertexIndex)
5910                   DeltaY=PanelY(FaceIndex,NextVertex)-PanelY(FaceIndex,VertexIndex)
5920                   DeltaZ=PanelZ(FaceIndex,NextVertex)-PanelZ(FaceIndex,VertexIndex)
5930                   Distance=SQR(DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ)
5940                   IF Distance<0.00001 THEN GOTO NextPanelEdge
5950                   Correction=(Distance-PanelEdgeLength(FaceIndex,VertexIndex))/Distance*0.48
5960                   PanelX(FaceIndex,VertexIndex)=PanelX(FaceIndex,VertexIndex)+DeltaX*Correction
5970                   PanelY(FaceIndex,VertexIndex)=PanelY(FaceIndex,VertexIndex)+DeltaY*Correction
5980                   PanelZ(FaceIndex,VertexIndex)=PanelZ(FaceIndex,VertexIndex)+DeltaZ*Correction
5990                   PanelX(FaceIndex,NextVertex)=PanelX(FaceIndex,NextVertex)-DeltaX*Correction
6000                   PanelY(FaceIndex,NextVertex)=PanelY(FaceIndex,NextVertex)-DeltaY*Correction
6010                   PanelZ(FaceIndex,NextVertex)=PanelZ(FaceIndex,NextVertex)-DeltaZ*Correction
6020               NextPanelEdge:
6030               NEXT VertexIndex

6040               FOR VertexIndex=0 TO 2
6050                   IF PanelY(FaceIndex,VertexIndex)>=0.28 THEN GOTO NextPanelGroundVertex
6060                   VelocityY=PanelY(FaceIndex,VertexIndex)-PreviousPanelY(FaceIndex,VertexIndex)
6070                   PanelY(FaceIndex,VertexIndex)=0.28 : PreviousPanelY(FaceIndex,VertexIndex)=0.28+MIN(0,VelocityY)*0.10
6080                   PreviousPanelX(FaceIndex,VertexIndex)=PanelX(FaceIndex,VertexIndex)-(PanelX(FaceIndex,VertexIndex)-PreviousPanelX(FaceIndex,VertexIndex))*0.74
6090                   PreviousPanelZ(FaceIndex,VertexIndex)=PanelZ(FaceIndex,VertexIndex)-(PanelZ(FaceIndex,VertexIndex)-PreviousPanelZ(FaceIndex,VertexIndex))*0.74
6100               NextPanelGroundVertex:
6110               NEXT VertexIndex
6120           NEXT PassIndex

6130           REM The piling solver owns sleep decisions when sampled supports are enabled.
6140           IF FastPilingEnabled=0 THEN
6150               Resting=1
6160               FOR VertexIndex=0 TO 2
6170                   IF PanelY(FaceIndex,VertexIndex)>0.31 OR ABS(PanelX(FaceIndex,VertexIndex)-PreviousPanelX(FaceIndex,VertexIndex))>0.0006 OR ABS(PanelZ(FaceIndex,VertexIndex)-PreviousPanelZ(FaceIndex,VertexIndex))>0.0006 THEN Resting=0
6180               NEXT VertexIndex
6190               IF Resting THEN PanelQuietTicks(FaceIndex)=PanelQuietTicks(FaceIndex)+1 ELSE PanelQuietTicks(FaceIndex)=0
6200               IF PanelQuietTicks(FaceIndex)>45 THEN PanelState(FaceIndex)=2
6210           ENDIF
6220       NextPanel:
6230       NEXT FaceIndex
6240   SUBEND

6250   DEF SUB DetachPanel(PanelIndex)
6260       LOCAL CenterX, CenterY, CenterZ, NormalX, NormalZ, AxisLength, AxisX, AxisZ, Spin
6270       LOCAL VertexIndex, SourceNode, VelocityX, VelocityY, VelocityZ, Speed, SpeedScale

6280       CenterX=(NodeX(PanelNodeA(PanelIndex))+NodeX(PanelNodeB(PanelIndex))+NodeX(PanelNodeC(PanelIndex)))/3
6290       CenterY=(NodeY(PanelNodeA(PanelIndex))+NodeY(PanelNodeB(PanelIndex))+NodeY(PanelNodeC(PanelIndex)))/3
6300       CenterZ=(NodeZ(PanelNodeA(PanelIndex))+NodeZ(PanelNodeB(PanelIndex))+NodeZ(PanelNodeC(PanelIndex)))/3
6310       NormalX=(NodeY(PanelNodeB(PanelIndex))-NodeY(PanelNodeA(PanelIndex)))*(NodeZ(PanelNodeC(PanelIndex))-NodeZ(PanelNodeA(PanelIndex)))-(NodeZ(PanelNodeB(PanelIndex))-NodeZ(PanelNodeA(PanelIndex)))*(NodeY(PanelNodeC(PanelIndex))-NodeY(PanelNodeA(PanelIndex)))
6320       NormalZ=(NodeX(PanelNodeB(PanelIndex))-NodeX(PanelNodeA(PanelIndex)))*(NodeY(PanelNodeC(PanelIndex))-NodeY(PanelNodeA(PanelIndex)))-(NodeY(PanelNodeB(PanelIndex))-NodeY(PanelNodeA(PanelIndex)))*(NodeX(PanelNodeC(PanelIndex))-NodeX(PanelNodeA(PanelIndex)))
6330       AxisLength=SQR(NormalX*NormalX+NormalZ*NormalZ) : AxisX=1 : AxisZ=0
6340       IF AxisLength>0.00001 THEN AxisX=NormalZ/AxisLength : AxisZ=-NormalX/AxisLength
6350       Spin=0.2*PhysicsStep : IF PanelIndex MOD 2 THEN Spin=-Spin

6360       FOR VertexIndex=0 TO 2
6370           IF VertexIndex=0 THEN
6380               SourceNode=PanelNodeA(PanelIndex)
6390           ELSEIF VertexIndex=1 THEN
6400               SourceNode=PanelNodeB(PanelIndex)
6410           ELSE
6420               SourceNode=PanelNodeC(PanelIndex)
6430           ENDIF
6440           PanelX(PanelIndex,VertexIndex)=NodeX(SourceNode) : PanelY(PanelIndex,VertexIndex)=NodeY(SourceNode) : PanelZ(PanelIndex,VertexIndex)=NodeZ(SourceNode)
6450           VelocityX=NodeX(SourceNode)-PreviousNodeX(SourceNode)-AxisZ*(NodeY(SourceNode)-CenterY)*Spin
6460           VelocityY=NodeY(SourceNode)-PreviousNodeY(SourceNode)+(AxisZ*(NodeX(SourceNode)-CenterX)-AxisX*(NodeZ(SourceNode)-CenterZ))*Spin
6470           VelocityZ=NodeZ(SourceNode)-PreviousNodeZ(SourceNode)+AxisX*(NodeY(SourceNode)-CenterY)*Spin
6480           Speed=SQR(VelocityX^2+VelocityY^2+VelocityZ^2) : SpeedScale=1
6490           IF Speed>PhysicsStep*12 THEN SpeedScale=PhysicsStep*12/Speed
6500           PreviousPanelX(PanelIndex,VertexIndex)=PanelX(PanelIndex,VertexIndex)-VelocityX*SpeedScale
6510           PreviousPanelY(PanelIndex,VertexIndex)=PanelY(PanelIndex,VertexIndex)-VelocityY*SpeedScale
6520           PreviousPanelZ(PanelIndex,VertexIndex)=PanelZ(PanelIndex,VertexIndex)-VelocityZ*SpeedScale
6530       NEXT VertexIndex
6540       PanelEdgeLength(PanelIndex,0)=LinkRestLength(PanelLink1(PanelIndex)) : PanelEdgeLength(PanelIndex,1)=LinkRestLength(PanelLink2(PanelIndex)) : PanelEdgeLength(PanelIndex,2)=LinkRestLength(PanelLink3(PanelIndex))
6550       PanelState(PanelIndex)=1 : PanelQuietTicks(PanelIndex)=0
6560   SUBEND
6570   REM ===============================================================
6580   REM Persistent rigid beam debris

6590   DEF SUB ResetBeams()
6600       IF BeamDebrisInitialized=0 THEN
6610           DIM BeamX(499,1),BeamY(499,1),BeamZ(499,1),PreviousBeamX(499,1),PreviousBeamY(499,1),PreviousBeamZ(499,1)
6620           DIM BeamLength(499),BeamState(499),BeamQuietTicks(499),BeamColor(499)
6630           BeamDebrisInitialized=1
6640       ENDIF
6650       MAT BeamState=0 : MAT BeamQuietTicks=0
6660       BeamRadius=0.055 : BeamFloorY=0.25+BeamRadius+0.005
6670   SUBEND

6680   DEF SUB StepBeams()
6690       LOCAL RodIndex, VertexIndex, PassIndex, Speed, SpeedScale, HitFraction, Lift, Rebound, Resting
6700       LOCAL DeltaX, DeltaY, DeltaZ, Distance, Correction
6710       LOCAL VelocityX(1),VelocityY(1),VelocityZ(1),GroundTouch(1)

6720       IF BeamDebrisInitialized=0 THEN EXIT SUB
6730       FOR RodIndex=0 TO LinkCount-1
6740           IF BeamState(RodIndex)<>1 THEN GOTO NextRod
6750           FOR VertexIndex=0 TO 1
6760               VelocityX(VertexIndex)=(BeamX(RodIndex,VertexIndex)-PreviousBeamX(RodIndex,VertexIndex))*0.994
6770               VelocityY(VertexIndex)=(BeamY(RodIndex,VertexIndex)-PreviousBeamY(RodIndex,VertexIndex))*0.994-9*PhysicsStep*PhysicsStep
6780               VelocityZ(VertexIndex)=(BeamZ(RodIndex,VertexIndex)-PreviousBeamZ(RodIndex,VertexIndex))*0.994
6790               Speed=SQR(VelocityX(VertexIndex)^2+VelocityY(VertexIndex)^2+VelocityZ(VertexIndex)^2)
6800               SpeedScale=1 : IF Speed>16*PhysicsStep THEN SpeedScale=16*PhysicsStep/Speed
6810               VelocityX(VertexIndex)=VelocityX(VertexIndex)*SpeedScale
6820               VelocityY(VertexIndex)=VelocityY(VertexIndex)*SpeedScale
6830               VelocityZ(VertexIndex)=VelocityZ(VertexIndex)*SpeedScale
6840               PreviousBeamX(RodIndex,VertexIndex)=BeamX(RodIndex,VertexIndex)
6850               PreviousBeamY(RodIndex,VertexIndex)=BeamY(RodIndex,VertexIndex)
6860               PreviousBeamZ(RodIndex,VertexIndex)=BeamZ(RodIndex,VertexIndex)
6870               GroundTouch(VertexIndex)=0
6880               BeamX(RodIndex,VertexIndex)=BeamX(RodIndex,VertexIndex)+VelocityX(VertexIndex)
6890               BeamY(RodIndex,VertexIndex)=BeamY(RodIndex,VertexIndex)+VelocityY(VertexIndex)
6900               BeamZ(RodIndex,VertexIndex)=BeamZ(RodIndex,VertexIndex)+VelocityZ(VertexIndex)

6910               IF BeamY(RodIndex,VertexIndex)<BeamFloorY THEN
6920                   REM Spend only the part of the step after impact in ground friction.
6930                   HitFraction=MAX(0,MIN(1,(PreviousBeamY(RodIndex,VertexIndex)-BeamFloorY)/MAX(0.0000001,-VelocityY(VertexIndex))))
6940                   BeamX(RodIndex,VertexIndex)=PreviousBeamX(RodIndex,VertexIndex)+VelocityX(VertexIndex)*(HitFraction+(1-HitFraction)*0.70)
6950                   BeamZ(RodIndex,VertexIndex)=PreviousBeamZ(RodIndex,VertexIndex)+VelocityZ(VertexIndex)*(HitFraction+(1-HitFraction)*0.70)
6960                   BeamY(RodIndex,VertexIndex)=BeamFloorY : GroundTouch(VertexIndex)=1
6970               ENDIF
6980           NEXT VertexIndex

6990           FOR PassIndex=1 TO MAX(2,SolverIterations)+2
7000               GOSUB ConstrainCurrentRod
7010               FOR VertexIndex=0 TO 1
7020                   IF BeamY(RodIndex,VertexIndex)<BeamFloorY THEN BeamY(RodIndex,VertexIndex)=BeamFloorY : GroundTouch(VertexIndex)=1
7030               NEXT VertexIndex
7040           NEXT PassIndex

7050           GOSUB ConstrainCurrentRod
7060           Lift=MAX(0,BeamFloorY-MIN(BeamY(RodIndex,0),BeamY(RodIndex,1)))
7070           IF Lift>0 THEN BeamY(RodIndex,0)=BeamY(RodIndex,0)+Lift : BeamY(RodIndex,1)=BeamY(RodIndex,1)+Lift

7080           FOR VertexIndex=0 TO 1
7090               IF BeamY(RodIndex,VertexIndex)<=BeamFloorY+0.000001 THEN GroundTouch(VertexIndex)=1
7100               IF GroundTouch(VertexIndex)=0 THEN GOTO NextRodGroundVertex
7110               PreviousBeamX(RodIndex,VertexIndex)=BeamX(RodIndex,VertexIndex)-VelocityX(VertexIndex)*0.70
7120               PreviousBeamZ(RodIndex,VertexIndex)=BeamZ(RodIndex,VertexIndex)-VelocityZ(VertexIndex)*0.70
7130               Rebound=0 : IF VelocityY(VertexIndex)<-0.7*PhysicsStep THEN Rebound=-VelocityY(VertexIndex)*0.15
7140               PreviousBeamY(RodIndex,VertexIndex)=BeamY(RodIndex,VertexIndex)-Rebound
7150           NextRodGroundVertex:
7160           NEXT VertexIndex

7170           IF FastPilingEnabled=0 THEN
7180               Resting=1
7190               FOR VertexIndex=0 TO 1
7200                   IF BeamY(RodIndex,VertexIndex)>BeamFloorY+0.002 OR ABS(BeamX(RodIndex,VertexIndex)-PreviousBeamX(RodIndex,VertexIndex))>0.0007 OR ABS(BeamY(RodIndex,VertexIndex)-PreviousBeamY(RodIndex,VertexIndex))>0.0007 OR ABS(BeamZ(RodIndex,VertexIndex)-PreviousBeamZ(RodIndex,VertexIndex))>0.0007 THEN Resting=0
7210               NEXT VertexIndex
7220               IF Resting THEN BeamQuietTicks(RodIndex)=BeamQuietTicks(RodIndex)+1 ELSE BeamQuietTicks(RodIndex)=0
7230               IF BeamQuietTicks(RodIndex)>=45 THEN
7240                   BeamState(RodIndex)=2
7250                   FOR VertexIndex=0 TO 1
7260                       PreviousBeamX(RodIndex,VertexIndex)=BeamX(RodIndex,VertexIndex)
7270                       PreviousBeamY(RodIndex,VertexIndex)=BeamY(RodIndex,VertexIndex)
7280                       PreviousBeamZ(RodIndex,VertexIndex)=BeamZ(RodIndex,VertexIndex)
7290                   NEXT VertexIndex
7300               ENDIF
7310           ENDIF
7320       NextRod:
7330       NEXT RodIndex
7340       EXIT SUB

7350       REM This private hot-loop helper shares the current rod's local scratch.
7360       ConstrainCurrentRod:
7370           DeltaX=BeamX(RodIndex,1)-BeamX(RodIndex,0)
7380           DeltaY=BeamY(RodIndex,1)-BeamY(RodIndex,0)
7390           DeltaZ=BeamZ(RodIndex,1)-BeamZ(RodIndex,0)
7400           Distance=SQR(DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ)
7410           IF Distance<0.0000001 THEN
7420               BeamX(RodIndex,0)=BeamX(RodIndex,0)-BeamLength(RodIndex)*0.5
7430               BeamX(RodIndex,1)=BeamX(RodIndex,1)+BeamLength(RodIndex)*0.5
7440               RETURN
7450           ENDIF
7460           Correction=(Distance-BeamLength(RodIndex))/Distance*0.5
7470           BeamX(RodIndex,0)=BeamX(RodIndex,0)+DeltaX*Correction
7480           BeamY(RodIndex,0)=BeamY(RodIndex,0)+DeltaY*Correction
7490           BeamZ(RodIndex,0)=BeamZ(RodIndex,0)+DeltaZ*Correction
7500           BeamX(RodIndex,1)=BeamX(RodIndex,1)-DeltaX*Correction
7510           BeamY(RodIndex,1)=BeamY(RodIndex,1)-DeltaY*Correction
7520           BeamZ(RodIndex,1)=BeamZ(RodIndex,1)-DeltaZ*Correction
7530           RETURN
7540   SUBEND

7550   DEF SUB DetachBeam(EdgeIndex)
7560       LOCAL FirstNode, SecondNode, CenterX, CenterY, CenterZ, DeltaX, DeltaY, DeltaZ, Distance
7570       LOCAL HorizontalLength, AxisX, AxisZ, Spin, VertexIndex, SourceNode
7580       LOCAL VelocityX, VelocityY, VelocityZ, Speed, SpeedScale

7590       IF BeamDebrisInitialized=0 THEN CALL ResetBeams
7600       IF EdgeIndex<0 OR EdgeIndex>=LinkCount THEN EXIT SUB
7610       IF BeamState(EdgeIndex)<>0 THEN EXIT SUB
7620       FirstNode=LinkNodeA(EdgeIndex) : SecondNode=LinkNodeB(EdgeIndex) : BeamColor(EdgeIndex)=NodeColor(SecondNode)
7630       CenterX=(NodeX(FirstNode)+NodeX(SecondNode))*0.5
7640       CenterY=(NodeY(FirstNode)+NodeY(SecondNode))*0.5
7650       CenterZ=(NodeZ(FirstNode)+NodeZ(SecondNode))*0.5
7660       DeltaX=NodeX(SecondNode)-NodeX(FirstNode) : DeltaY=NodeY(SecondNode)-NodeY(FirstNode) : DeltaZ=NodeZ(SecondNode)-NodeZ(FirstNode)
7670       Distance=SQR(DeltaX^2+DeltaY^2+DeltaZ^2)
7680       BeamLength(EdgeIndex)=MAX(0.001,LinkRestLength(EdgeIndex))
7690       IF Distance<0.65*BeamLength(EdgeIndex) OR Distance>1.35*BeamLength(EdgeIndex) THEN BeamLength(EdgeIndex)=MAX(0.001,Distance)
7700       HorizontalLength=SQR(DeltaX^2+DeltaZ^2) : AxisX=1 : AxisZ=0
7710       IF HorizontalLength>0.00001 THEN AxisX=DeltaZ/HorizontalLength : AxisZ=-DeltaX/HorizontalLength
7720       Spin=0.2*PhysicsStep : IF EdgeIndex MOD 2 THEN Spin=-Spin

7730       FOR VertexIndex=0 TO 1
7740           SourceNode=IIF(VertexIndex=0,FirstNode,SecondNode)
7750           BeamX(EdgeIndex,VertexIndex)=NodeX(SourceNode)
7760           BeamY(EdgeIndex,VertexIndex)=NodeY(SourceNode)
7770           BeamZ(EdgeIndex,VertexIndex)=NodeZ(SourceNode)
7780           VelocityX=NodeX(SourceNode)-PreviousNodeX(SourceNode)-AxisZ*(NodeY(SourceNode)-CenterY)*Spin
7790           VelocityY=NodeY(SourceNode)-PreviousNodeY(SourceNode)+(AxisZ*(NodeX(SourceNode)-CenterX)-AxisX*(NodeZ(SourceNode)-CenterZ))*Spin
7800           VelocityZ=NodeZ(SourceNode)-PreviousNodeZ(SourceNode)+AxisX*(NodeY(SourceNode)-CenterY)*Spin
7810           Speed=SQR(VelocityX^2+VelocityY^2+VelocityZ^2) : SpeedScale=1
7820           IF Speed>16*PhysicsStep THEN SpeedScale=16*PhysicsStep/Speed
7830           PreviousBeamX(EdgeIndex,VertexIndex)=BeamX(EdgeIndex,VertexIndex)-VelocityX*SpeedScale
7840           PreviousBeamY(EdgeIndex,VertexIndex)=BeamY(EdgeIndex,VertexIndex)-VelocityY*SpeedScale
7850           PreviousBeamZ(EdgeIndex,VertexIndex)=BeamZ(EdgeIndex,VertexIndex)-VelocityZ*SpeedScale
7860       NEXT VertexIndex
7870       BeamState(EdgeIndex)=1 : BeamQuietTicks(EdgeIndex)=0
7880   SUBEND
7890   REM === Fast rubble supports ==================================================
7900   REM Settled lines and triangles become a sampled height surface. Moving pieces
7910   REM keep their shape and probe that surface; no all-pairs contact solver is used.

7920   DEF SUB ResetRubble()
7930       IF RubbleInitialized = 0 THEN
7940           DIM RubbleSurfaceHeight(240,240), RubbleDeposited(595), NextRubbleInBucket(595), HeightBucketHead(127)
7950           DIM LastRubbleX(595,2), LastRubbleY(595,2), LastRubbleZ(595,2), HasLastRubblePose(595)
7960           RubbleInitialized = 1
7970       ENDIF
7980       MAT RubbleSurfaceHeight = 0.25 : MAT RubbleDeposited = 0 : MAT HasLastRubblePose = 0
7990       HighestRubbleSurface = 0.25 : FastPilingEnabled = 1
8000       RubbleBodyCount = 0 : RubbleContactCount = 0 : SettledRubbleCount = 0 : RubbleDepositCount = 0
8010   SUBEND

8020   DEF SUB StepRubble()
8030       LOCAL bodyId, panelId, faceId, bucket, nextBody, vertex, vertexCount, radius, floorY
8040       LOCAL lowestY, highestY, probe, probeCount, segments, contactCount, iteration
8050       LOCAL weightA, weightB, weightC, sampleX, sampleY, sampleZ, priorY, surfaceY, targetY
8060       LOCAL cellX, cellZ, denominator, penetration, correction, velocity, impulse, lift
8070       LOCAL maxVelocity, steep, stable, testSupport, restTicks, newState, depositsThisStep
8080       LOCAL supportCount, supportX, supportZ, minA, minB, minC, maxA, maxB, maxC
8090       LOCAL topVertex, kickX, kickZ, kickLength
8100       LOCAL centerX, centerY, centerZ, axisX, axisY, axisZ, axisLength
8110       LOCAL offsetX, offsetY, offsetZ, projection, sideX, sideY, sideZ, sideLength
8120       LOCAL firstLength, secondOffset, triangleHeight, localX, localY, newX, newY, newZ
8130       LOCAL pose(2,2), oldPose(2,2), edgeLengths(2), bodyFrame(2,2)
8140       LOCAL weights(15,2), targets(15), contacts(15)

8150       IF FastPilingEnabled = 0 THEN EXIT SUB
8160       MAT HeightBucketHead = -1
8170       RubbleBodyCount = 0 : RubbleContactCount = 0 : MovingRubbleCount = 0 : SupportProbeCount = 0 : depositsThisStep = 0

8180       REM Put moving fragments in low-to-high buckets; stamp pre-existing sleepers.
8190       FOR bodyId = 0 TO LinkCount - 1
8200           IF BeamState(bodyId) <> 0 THEN
8210               RubbleBodyCount = RubbleBodyCount + 1
8220               IF BeamState(bodyId) = 2 AND RubbleDeposited(bodyId) = 0 AND depositsThisStep < 2 THEN GOSUB LoadPose : GOSUB DepositPose
8230               IF BeamState(bodyId) = 1 THEN
8240                   lowestY = MIN(BeamY(bodyId,0), BeamY(bodyId,1))
8250                   bucket = MAX(0, MIN(127, INT((lowestY - 0.25) * 4)))
8260                   NextRubbleInBucket(bodyId) = HeightBucketHead(bucket) : HeightBucketHead(bucket) = bodyId
8270                   MovingRubbleCount = MovingRubbleCount + 1
8280               ENDIF
8290           ENDIF
8300       NEXT bodyId
8310       FOR panelId = 0 TO PanelCount - 1
8320           bodyId = 500 + panelId
8330           IF PanelState(panelId) <> 0 THEN
8340               RubbleBodyCount = RubbleBodyCount + 1
8350               IF PanelState(panelId) = 2 AND RubbleDeposited(bodyId) = 0 AND depositsThisStep < 2 THEN GOSUB LoadPose : GOSUB DepositPose
8360               IF PanelState(panelId) = 1 THEN
8370                   lowestY = MIN(PanelY(panelId,0), PanelY(panelId,1), PanelY(panelId,2))
8380                   bucket = MAX(0, MIN(127, INT((lowestY - 0.25) * 4)))
8390                   NextRubbleInBucket(bodyId) = HeightBucketHead(bucket) : HeightBucketHead(bucket) = bodyId
8400                   MovingRubbleCount = MovingRubbleCount + 1
8410               ENDIF
8420           ENDIF
8430       NEXT panelId

8440       FOR bucket = 0 TO 127
8450           bodyId = HeightBucketHead(bucket)
8460           WHILE bodyId >= 0
8470               nextBody = NextRubbleInBucket(bodyId)
8480               GOSUB LoadPose : GOSUB GatherSupports : GOSUB SolveSupports : GOSUB SettlePose
8490               bodyId = nextBody
8500           WEND
8510       NEXT bucket
8520       EXIT SUB

8530   LoadPose:
8540       vertexCount = 2 : radius = 0.055 : floorY = BeamFloorY
8550       IF bodyId >= 500 THEN vertexCount = 3 : radius = 0.03 : floorY = 0.28 : faceId = bodyId - 500
8560       FOR vertex = 0 TO vertexCount - 1
8570           IF vertexCount = 2 THEN
8580               pose(vertex,0) = BeamX(bodyId,vertex) : pose(vertex,1) = BeamY(bodyId,vertex) : pose(vertex,2) = BeamZ(bodyId,vertex)
8590               oldPose(vertex,0) = PreviousBeamX(bodyId,vertex) : oldPose(vertex,1) = PreviousBeamY(bodyId,vertex) : oldPose(vertex,2) = PreviousBeamZ(bodyId,vertex)
8600               edgeLengths(0) = BeamLength(bodyId)
8610           ELSE
8620               pose(vertex,0) = PanelX(faceId,vertex) : pose(vertex,1) = PanelY(faceId,vertex) : pose(vertex,2) = PanelZ(faceId,vertex)
8630               oldPose(vertex,0) = PreviousPanelX(faceId,vertex) : oldPose(vertex,1) = PreviousPanelY(faceId,vertex) : oldPose(vertex,2) = PreviousPanelZ(faceId,vertex)
8640               edgeLengths(vertex) = PanelEdgeLength(faceId,vertex)
8650           ENDIF
8660       NEXT vertex
8670       RETURN

8680   GatherSupports:
8690       REM The weights identify endpoints, edge midpoints and the triangle centre.
8700       contactCount = 0 : probeCount = 7
8710       IF vertexCount = 2 THEN segments = MIN(12, MAX(2, 2 * INT(edgeLengths(0) + 0.99))) : probeCount = segments + 1
8720       FOR probe = 0 TO probeCount - 1
8730           contacts(probe) = 0
8740           weights(probe,0) = 0 : weights(probe,1) = 0 : weights(probe,2) = 0
8750           IF vertexCount = 2 THEN
8760               weights(probe,1) = probe / segments : weights(probe,0) = 1 - weights(probe,1)
8770           ELSEIF probe = 0 THEN
8780               weights(probe,0) = 1
8790           ELSEIF probe = 1 THEN
8800               weights(probe,1) = 1
8810           ELSEIF probe = 2 THEN
8820               weights(probe,2) = 1
8830           ELSEIF probe = 3 THEN
8840               weights(probe,0) = 0.5 : weights(probe,1) = 0.5
8850           ELSEIF probe = 4 THEN
8860               weights(probe,1) = 0.5 : weights(probe,2) = 0.5
8870           ELSEIF probe = 5 THEN
8880               weights(probe,2) = 0.5 : weights(probe,0) = 0.5
8890           ELSE
8900               weights(probe,0) = 1/3 : weights(probe,1) = 1/3 : weights(probe,2) = 1/3
8910           ENDIF
8920           weightA = weights(probe,0) : weightB = weights(probe,1) : weightC = weights(probe,2)
8930           sampleX = weightA * pose(0,0) + weightB * pose(1,0) + weightC * pose(2,0)
8940           sampleY = weightA * pose(0,1) + weightB * pose(1,1) + weightC * pose(2,1)
8950           sampleZ = weightA * pose(0,2) + weightB * pose(1,2) + weightC * pose(2,2)
8960           priorY = weightA * oldPose(0,1) + weightB * oldPose(1,1) + weightC * oldPose(2,1)
8970           IF HasLastRubblePose(bodyId) THEN priorY = weightA * LastRubbleY(bodyId,0) + weightB * LastRubbleY(bodyId,1) + weightC * LastRubbleY(bodyId,2)
8980           surfaceY = 0.25 : cellX = INT((sampleX + 24) * 5 + 0.5) : cellZ = INT((sampleZ + 24) * 5 + 0.5)
8990           IF cellX >= 0 AND cellX <= 240 AND cellZ >= 0 AND cellZ <= 240 THEN surfaceY = RubbleSurfaceHeight(cellX,cellZ)
9000           targetY = floorY : IF surfaceY > 0.251 THEN targetY = surfaceY + radius + 0.004
9010           SupportProbeCount = SupportProbeCount + 1 : targets(probe) = targetY
9020           IF sampleY > targetY + 0.003 THEN GOTO NextSupportProbe
9030           IF surfaceY > 0.251 AND priorY < targetY - 0.025 AND targetY - sampleY > 0.13 THEN GOTO NextSupportProbe
9040           contacts(probe) = 1 : contactCount = contactCount + 1 : RubbleContactCount = RubbleContactCount + 1
9050   NextSupportProbe:
9060       NEXT probe
9070       RETURN

9080   SolveSupports:
9090       IF contactCount = 0 THEN GOSUB ConstrainPose : RETURN
9100       REM Reuse the gathered support heights in both passes.
9110       FOR iteration = 1 TO 2
9120           FOR probe = 0 TO probeCount - 1
9130               IF contacts(probe) = 0 THEN GOTO NextContact
9140               weightA = weights(probe,0) : weightB = weights(probe,1) : weightC = weights(probe,2)
9150               denominator = weightA * weightA + weightB * weightB + weightC * weightC
9160               sampleY = weightA * pose(0,1) + weightB * pose(1,1) + weightC * pose(2,1)
9170               penetration = targets(probe) - sampleY
9180               IF penetration > 0 THEN
9190                   correction = MIN(0.3, penetration) * 0.88 / denominator
9200                   pose(0,1) = pose(0,1) + correction * weightA : pose(1,1) = pose(1,1) + correction * weightB : pose(2,1) = pose(2,1) + correction * weightC
9210                   oldPose(0,1) = oldPose(0,1) + correction * weightA : oldPose(1,1) = oldPose(1,1) + correction * weightB : oldPose(2,1) = oldPose(2,1) + correction * weightC
9220               ENDIF
9230               velocity = weightA * (pose(0,1) - oldPose(0,1)) + weightB * (pose(1,1) - oldPose(1,1)) + weightC * (pose(2,1) - oldPose(2,1))
9240               IF velocity < 0 THEN
9250                   impulse = velocity / denominator : IF velocity < -0.007 THEN impulse = impulse * 1.06
9260                   oldPose(0,1) = oldPose(0,1) + impulse * weightA : oldPose(1,1) = oldPose(1,1) + impulse * weightB : oldPose(2,1) = oldPose(2,1) + impulse * weightC
9270               ENDIF
9280   NextContact:
9290           NEXT probe
9300           GOSUB ConstrainPose
9310       NEXT iteration
9320       lift = 0
9330       FOR probe = 0 TO probeCount - 1
9340           IF contacts(probe) THEN lift = MAX(lift, targets(probe) - weights(probe,0) * pose(0,1) - weights(probe,1) * pose(1,1) - weights(probe,2) * pose(2,1))
9350       NEXT probe
9360       IF lift > 0 THEN
9370           FOR vertex = 0 TO vertexCount - 1
9380               pose(vertex,1) = pose(vertex,1) + lift : oldPose(vertex,1) = oldPose(vertex,1) + lift
9390           NEXT vertex
9400       ENDIF
9410       FOR vertex = 0 TO vertexCount - 1
9420           oldPose(vertex,0) = pose(vertex,0) - (pose(vertex,0) - oldPose(vertex,0)) * 0.72
9430           oldPose(vertex,2) = pose(vertex,2) - (pose(vertex,2) - oldPose(vertex,2)) * 0.72
9440           oldPose(vertex,1) = pose(vertex,1) - (pose(vertex,1) - oldPose(vertex,1)) * 0.85
9450       NEXT vertex
9460       RETURN

9470   SettlePose:
9480       maxVelocity = 0 : highestY = MAX(pose(0,1), pose(1,1)) : lowestY = MIN(pose(0,1), pose(1,1))
9490       IF vertexCount = 3 THEN highestY = MAX(highestY, pose(2,1)) : lowestY = MIN(lowestY, pose(2,1))
9500       steep = highestY - lowestY > 0.80 * edgeLengths(0)
9510       IF vertexCount = 3 THEN steep = ABS(bodyFrame(1,0) * bodyFrame(2,2) - bodyFrame(1,2) * bodyFrame(2,0)) < 0.25
9520       FOR vertex = 0 TO vertexCount - 1
9530           maxVelocity = MAX(maxVelocity, ABS(pose(vertex,0) - oldPose(vertex,0)), ABS(pose(vertex,1) - oldPose(vertex,1)), ABS(pose(vertex,2) - oldPose(vertex,2)))
9540       NEXT vertex
9550       stable = 0 : testSupport = 0
9560       IF contactCount > 0 AND (maxVelocity < 0.0028 OR steep) THEN testSupport = 1 : GOSUB CheckSupportCentre
9570       IF testSupport AND stable = 0 THEN GOSUB TipUnsupportedPose
9580       restTicks = IIF(vertexCount = 2, BeamQuietTicks(bodyId), PanelQuietTicks(faceId))
9590       IF contactCount > 0 AND steep = 0 AND stable AND maxVelocity < 0.0028 THEN restTicks = restTicks + 1 ELSE restTicks = 0
9600       newState = IIF(restTicks >= 16, 2, 1)
9610       FOR vertex = 0 TO vertexCount - 1
9620           IF newState = 2 THEN oldPose(vertex,0) = pose(vertex,0) : oldPose(vertex,1) = pose(vertex,1) : oldPose(vertex,2) = pose(vertex,2)
9630           LastRubbleX(bodyId,vertex) = pose(vertex,0) : LastRubbleY(bodyId,vertex) = pose(vertex,1) : LastRubbleZ(bodyId,vertex) = pose(vertex,2)
9640           IF vertexCount = 2 THEN
9650               BeamX(bodyId,vertex) = pose(vertex,0) : BeamY(bodyId,vertex) = pose(vertex,1) : BeamZ(bodyId,vertex) = pose(vertex,2)
9660               PreviousBeamX(bodyId,vertex) = oldPose(vertex,0) : PreviousBeamY(bodyId,vertex) = oldPose(vertex,1) : PreviousBeamZ(bodyId,vertex) = oldPose(vertex,2)
9670           ELSE
9680               PanelX(faceId,vertex) = pose(vertex,0) : PanelY(faceId,vertex) = pose(vertex,1) : PanelZ(faceId,vertex) = pose(vertex,2)
9690               PreviousPanelX(faceId,vertex) = oldPose(vertex,0) : PreviousPanelY(faceId,vertex) = oldPose(vertex,1) : PreviousPanelZ(faceId,vertex) = oldPose(vertex,2)
9700           ENDIF
9710       NEXT vertex
9720       HasLastRubblePose(bodyId) = 1
9730       IF vertexCount = 2 THEN BeamState(bodyId) = newState : BeamQuietTicks(bodyId) = restTicks ELSE PanelState(faceId) = newState : PanelQuietTicks(faceId) = restTicks
9740       IF newState = 2 AND RubbleDeposited(bodyId) = 0 AND depositsThisStep < 2 THEN GOSUB DepositPose
9750       RETURN

9760   CheckSupportCentre:
9770       REM Only contacts still touching after projection count as support.
9780       supportCount = 0 : supportX = 0 : supportZ = 0
9790       minA = 1 : minB = 1 : minC = 1 : maxA = 0 : maxB = 0 : maxC = 0
9800       FOR probe = 0 TO probeCount - 1
9810           IF contacts(probe) = 0 THEN GOTO NextCentreProbe
9820           sampleY = weights(probe,0) * pose(0,1) + weights(probe,1) * pose(1,1) + weights(probe,2) * pose(2,1)
9830           IF sampleY > targets(probe) + 0.012 THEN GOTO NextCentreProbe
9840           supportCount = supportCount + 1
9850           supportX = supportX + weights(probe,0) * pose(0,0) + weights(probe,1) * pose(1,0) + weights(probe,2) * pose(2,0)
9860           supportZ = supportZ + weights(probe,0) * pose(0,2) + weights(probe,1) * pose(1,2) + weights(probe,2) * pose(2,2)
9870           minA = MIN(minA, weights(probe,0)) : maxA = MAX(maxA, weights(probe,0))
9880           minB = MIN(minB, weights(probe,1)) : maxB = MAX(maxB, weights(probe,1))
9890           minC = MIN(minC, weights(probe,2)) : maxC = MAX(maxC, weights(probe,2))
9900   NextCentreProbe:
9910       NEXT probe
9920       IF supportCount = 0 THEN supportX = bodyFrame(0,0) : supportZ = bodyFrame(0,2) : RETURN
9930       supportX = supportX / supportCount : supportZ = supportZ / supportCount
9940       IF vertexCount = 2 THEN stable = minB <= 0.55 AND maxB >= 0.45 : RETURN
9950       stable = minA <= 1/3 + 0.025 AND maxA >= 1/3 - 0.025 AND minB <= 1/3 + 0.025 AND maxB >= 1/3 - 0.025 AND minC <= 1/3 + 0.025 AND maxC >= 1/3 - 0.025
9960       RETURN

9970   TipUnsupportedPose:
9980       REM Break precarious balance in the direction away from the support.
9990       topVertex = 0
10000      FOR vertex = 1 TO vertexCount - 1
10010          IF pose(vertex,1) > pose(topVertex,1) + 0.01 THEN topVertex = vertex : GOTO NextTipVertex
10020          IF ABS(pose(vertex,1) - pose(topVertex,1)) <= 0.01 THEN
10030              IF (pose(vertex,0) - supportX)^2 + (pose(vertex,2) - supportZ)^2 > (pose(topVertex,0) - supportX)^2 + (pose(topVertex,2) - supportZ)^2 THEN topVertex = vertex
10040          ENDIF
10050  NextTipVertex:
10060      NEXT vertex
10070      kickX = pose(topVertex,0) - supportX : kickZ = pose(topVertex,2) - supportZ
10080      kickLength = SQR(kickX * kickX + kickZ * kickZ)
10090      IF kickLength < 0.0001 AND vertexCount = 3 THEN
10100          kickX = bodyFrame(1,1) * bodyFrame(2,2) - bodyFrame(1,2) * bodyFrame(2,1)
10110          kickZ = bodyFrame(1,0) * bodyFrame(2,1) - bodyFrame(1,1) * bodyFrame(2,0)
10120          kickLength = SQR(kickX * kickX + kickZ * kickZ)
10130      ENDIF
10140      IF kickLength < 0.0001 THEN kickX = 1 : kickZ = 0.37 : kickLength = SQR(1 + 0.37^2)
10150      oldPose(topVertex,0) = oldPose(topVertex,0) - kickX / kickLength * 0.22 * PhysicsStep
10160      oldPose(topVertex,2) = oldPose(topVertex,2) - kickZ / kickLength * 0.22 * PhysicsStep
10170      RETURN

10180  DepositPose:
10190      IF RubbleDeposited(bodyId) THEN RETURN
10200      IF vertexCount = 2 THEN
10210          CALL StampRubbleSegment(RubbleSurfaceHeight, pose, 0, 1, radius)
10220      ELSE
10230          CALL StampRubbleSegment(RubbleSurfaceHeight, pose, 0, 1, radius)
10240          CALL StampRubbleSegment(RubbleSurfaceHeight, pose, 1, 2, radius)
10250          CALL StampRubbleSegment(RubbleSurfaceHeight, pose, 2, 0, radius)
10260          CALL StampRubbleTriangle(RubbleSurfaceHeight, pose, radius)
10270      ENDIF
10280      RubbleDeposited(bodyId) = 1 : depositsThisStep = depositsThisStep + 1
10290      RubbleDepositCount = RubbleDepositCount + 1 : SettledRubbleCount = SettledRubbleCount + 1
10300      RETURN

10310  ConstrainPose:
10320      REM Pose rows are vertices; columns are X, Y, Z. bodyFrame retains the
10330      REM centroid and both in-plane axes for the support and tipping checks.

10340      centerX = 0 : centerY = 0 : centerZ = 0
10350      FOR vertex = 0 TO vertexCount - 1
10360          centerX = centerX + pose(vertex,0) / vertexCount
10370          centerY = centerY + pose(vertex,1) / vertexCount
10380          centerZ = centerZ + pose(vertex,2) / vertexCount
10390      NEXT vertex
10400      axisX = pose(1,0) - pose(0,0) : axisY = pose(1,1) - pose(0,1) : axisZ = pose(1,2) - pose(0,2)
10410      axisLength = SQR(axisX * axisX + axisY * axisY + axisZ * axisZ)
10420      IF axisLength < 0.000001 THEN axisX = 1 : axisY = 0 : axisZ = 0 : axisLength = 1
10430      axisX = axisX / axisLength : axisY = axisY / axisLength : axisZ = axisZ / axisLength
10440      firstLength = edgeLengths(0)
10450      bodyFrame(0,0) = centerX : bodyFrame(0,1) = centerY : bodyFrame(0,2) = centerZ
10460      bodyFrame(1,0) = axisX : bodyFrame(1,1) = axisY : bodyFrame(1,2) = axisZ

10470      IF vertexCount = 3 THEN
10480          offsetX = pose(2,0) - pose(0,0) : offsetY = pose(2,1) - pose(0,1) : offsetZ = pose(2,2) - pose(0,2)
10490          projection = offsetX * axisX + offsetY * axisY + offsetZ * axisZ
10500          sideX = offsetX - projection * axisX : sideY = offsetY - projection * axisY : sideZ = offsetZ - projection * axisZ
10510          sideLength = SQR(sideX * sideX + sideY * sideY + sideZ * sideZ)
10520          IF sideLength < 0.000001 THEN sideX = -axisZ : sideY = 0 : sideZ = axisX : sideLength = SQR(sideX * sideX + sideZ * sideZ)
10530          IF sideLength < 0.000001 THEN sideX = 1 : sideY = 0 : sideZ = 0 : sideLength = 1
10540          sideX = sideX / sideLength : sideY = sideY / sideLength : sideZ = sideZ / sideLength
10550          secondOffset = (edgeLengths(2)^2 + firstLength^2 - edgeLengths(1)^2) / (2 * MAX(0.000001, firstLength))
10560          triangleHeight = SQR(MAX(0, edgeLengths(2)^2 - secondOffset^2))
10570          bodyFrame(2,0) = sideX : bodyFrame(2,1) = sideY : bodyFrame(2,2) = sideZ
10580          FOR vertex = 0 TO 2
10590              localX = -(firstLength + secondOffset) / 3 : localY = -triangleHeight / 3
10600              IF vertex = 1 THEN localX = localX + firstLength
10610              IF vertex = 2 THEN localX = localX + secondOffset : localY = localY + triangleHeight
10620              newX = centerX + axisX * localX + sideX * localY
10630              newY = centerY + axisY * localX + sideY * localY
10640              newZ = centerZ + axisZ * localX + sideZ * localY
10650              GOSUB PlaceVertex
10660          NEXT vertex
10670      ELSE
10680          FOR vertex = 0 TO 1
10690              localX = (vertex - 0.5) * firstLength
10700              newX = centerX + axisX * localX : newY = centerY + axisY * localX : newZ = centerZ + axisZ * localX
10710              GOSUB PlaceVertex
10720          NEXT vertex
10730      ENDIF
10740      lift = 0
10750      FOR vertex = 0 TO vertexCount - 1 : lift = MAX(lift, floorY - pose(vertex,1)) : NEXT vertex
10760      IF lift > 0 THEN
10770          FOR vertex = 0 TO vertexCount - 1
10780              pose(vertex,1) = pose(vertex,1) + lift : oldPose(vertex,1) = oldPose(vertex,1) + lift
10790          NEXT vertex
10800      ENDIF
10810      RETURN

10820  PlaceVertex:
10830      REM Move both poses together: correcting shape must not create velocity.
10840      oldPose(vertex,0) = oldPose(vertex,0) + newX - pose(vertex,0)
10850      oldPose(vertex,1) = oldPose(vertex,1) + newY - pose(vertex,1)
10860      oldPose(vertex,2) = oldPose(vertex,2) + newZ - pose(vertex,2)
10870      pose(vertex,0) = newX : pose(vertex,1) = newY : pose(vertex,2) = newZ
10880      RETURN
10890  SUBEND

10900  DEF SUB StampRubbleSegment(HeightGrid, Pose, VertexA, VertexB, Radius)
10910      LOCAL deltaX, deltaY, deltaZ, steps, sample, fraction, sampleX, sampleY, sampleZ
10920      LOCAL cellX, cellZ
10930      deltaX = Pose(VertexB,0) - Pose(VertexA,0) : deltaZ = Pose(VertexB,2) - Pose(VertexA,2) : deltaY = Pose(VertexB,1) - Pose(VertexA,1)
10940      steps = MAX(1, INT(MAX(ABS(deltaX), ABS(deltaZ)) * 10) + 1)
10950      FOR sample = 0 TO steps
10960          fraction = sample / steps
10970          sampleX = Pose(VertexA,0) + deltaX * fraction : sampleZ = Pose(VertexA,2) + deltaZ * fraction
10980          sampleY = Pose(VertexA,1) + deltaY * fraction + Radius
10990          GOSUB StampSample
11000      NEXT sample
11010      EXIT SUB

11020  StampSample:
11030      REM Reuse this segment's locals for every sample; the height map stays by reference.
11040      cellX = INT((sampleX + 24) * 5 + 0.5) : cellZ = INT((sampleZ + 24) * 5 + 0.5)
11050      IF cellX < 0 OR cellX > 240 OR cellZ < 0 OR cellZ > 240 THEN RETURN
11060      HeightGrid(cellX,cellZ) = MAX(HeightGrid(cellX,cellZ), sampleY)
11070      HighestRubbleSurface = MAX(HighestRubbleSurface, sampleY)
11080      RETURN
11090  SUBEND

11100  DEF SUB StampRubbleTriangle(HeightGrid, Pose, Radius)
11110      LOCAL edgeAX, edgeAZ, edgeBX, edgeBZ, determinant, minX, maxX, minZ, maxZ
11120      LOCAL cellX, cellZ, offsetX, offsetZ, weightB, weightC, sampleY
11130      edgeAX = Pose(1,0) - Pose(0,0) : edgeAZ = Pose(1,2) - Pose(0,2)
11140      edgeBX = Pose(2,0) - Pose(0,0) : edgeBZ = Pose(2,2) - Pose(0,2)
11150      determinant = edgeAX * edgeBZ - edgeAZ * edgeBX
11160      IF ABS(determinant) < 0.00001 THEN EXIT SUB
11170      minX = MAX(0, INT((MIN(Pose(0,0), Pose(1,0), Pose(2,0)) + 24) * 5))
11180      maxX = MIN(240, INT((MAX(Pose(0,0), Pose(1,0), Pose(2,0)) + 24) * 5) + 1)
11190      minZ = MAX(0, INT((MIN(Pose(0,2), Pose(1,2), Pose(2,2)) + 24) * 5))
11200      maxZ = MIN(240, INT((MAX(Pose(0,2), Pose(1,2), Pose(2,2)) + 24) * 5) + 1)
11210      FOR cellX = minX TO maxX
11220          offsetX = cellX * 0.2 - 24 - Pose(0,0)
11230          FOR cellZ = minZ TO maxZ
11240              offsetZ = cellZ * 0.2 - 24 - Pose(0,2)
11250              weightB = (offsetX * edgeBZ - offsetZ * edgeBX) / determinant
11260              weightC = (edgeAX * offsetZ - edgeAZ * offsetX) / determinant
11270              IF weightB >= 0 AND weightC >= 0 AND weightB + weightC <= 1 THEN
11280                  sampleY = Pose(0,1) + (Pose(1,1) - Pose(0,1)) * weightB + (Pose(2,1) - Pose(0,1)) * weightC + Radius
11290                  HeightGrid(cellX,cellZ) = MAX(HeightGrid(cellX,cellZ), sampleY)
11300                  HighestRubbleSurface = MAX(HighestRubbleSurface, sampleY)
11310              ENDIF
11320          NEXT cellZ
11330      NEXT cellX
11340  SUBEND

11350  REM === Drawing =============================================================
11360  DEF SUB RenderWorld()
11370      LOCAL nodeId, worldX, worldY, worldZ, deltaX, deltaY, deltaZ, depth
11380      LOCAL pointX, pointY, pointDepth
11390      CLG OFFSCREEN : MAT DepthBuffer=0
11400      CALL DrawSky : CALL DrawDais(DepthBuffer)
11410      FOR nodeId=0 TO NodeCount-1
11420          worldX=NodeX(nodeId) : worldY=NodeY(nodeId) : worldZ=NodeZ(nodeId)
11430          GOSUB ProjectNode
11440          NodeScreenX(nodeId)=pointX : NodeScreenY(nodeId)=pointY : NodeInverseDepth(nodeId)=pointDepth
11450      NEXT nodeId
11460      CALL DrawPanels(DepthBuffer,NodeScreenX,NodeScreenY,NodeInverseDepth)
11470      CALL DrawIntactLinks(DepthBuffer,NodeScreenX,NodeScreenY,NodeInverseDepth)
11480      CALL RenderBeamDebris(DepthBuffer)
11490      CALL DrawJoints(DepthBuffer,NodeScreenX,NodeScreenY,NodeInverseDepth)
11500      CALL DrawSparks(DepthBuffer)
11510      EXIT SUB

11520  ProjectNode:
11530      REM Reuse this frame's local scratch instead of binding a SUB per vertex.
11540      deltaX=worldX-CameraX : deltaY=worldY-CameraY : deltaZ=worldZ-CameraZ
11550      depth=deltaX*CameraForwardX+deltaY*CameraForwardY+deltaZ*CameraForwardZ
11560      pointX=0 : pointY=0 : pointDepth=0
11570      IF depth<0.35 THEN RETURN
11580      pointDepth=1/depth
11590      pointX=320+FocalLength*(deltaX*CameraRightX+deltaZ*CameraRightZ)*pointDepth
11600      pointY=ViewCenterY+FocalLength*(deltaX*CameraUpX+deltaY*CameraUpY+deltaZ*CameraUpZ)*pointDepth
11610      RETURN
11620  SUBEND

11630  DEF SUB DrawPanels(depthBuffer,screenX,screenY,inverseDepth)
11640      LOCAL panelId, vertex, nodeId, colorIndex, vertices(2,2)
11650      LOCAL worldX, worldY, worldZ, deltaX, deltaY, deltaZ, depth, pointX, pointY, pointDepth
11660      FOR panelId=0 TO PanelCount-1
11670          IF PanelState(panelId)=0 THEN
11680              IF LinkActive(PanelLink1(panelId))=0 OR LinkActive(PanelLink2(panelId))=0 OR LinkActive(PanelLink3(panelId))=0 THEN GOTO NextPanel
11690              FOR vertex=0 TO 2
11700                  nodeId=IIF(vertex=0,PanelNodeA(panelId),IIF(vertex=1,PanelNodeB(panelId),PanelNodeC(panelId)))
11710                  vertices(vertex,0)=screenX(nodeId) : vertices(vertex,1)=screenY(nodeId) : vertices(vertex,2)=inverseDepth(nodeId)
11720              NEXT vertex
11730          ELSE
11740              FOR vertex=0 TO 2
11750                  worldX=PanelX(panelId,vertex) : worldY=PanelY(panelId,vertex) : worldZ=PanelZ(panelId,vertex)
11760                  GOSUB ProjectPanelVertex
11770                  vertices(vertex,0)=pointX : vertices(vertex,1)=pointY : vertices(vertex,2)=pointDepth
11780              NEXT vertex
11790          ENDIF
11800          IF MIN(vertices(0,2),MIN(vertices(1,2),vertices(2,2)))<=0 THEN GOTO NextPanel
11810          colorIndex=1+(panelId MOD 3)
11820          GTRIANGLE depthBuffer,vertices(0,0),vertices(0,1),vertices(0,2),PaletteRed(colorIndex)*0.30,PaletteGreen(colorIndex)*0.30,PaletteBlue(colorIndex)*0.46,vertices(1,0),vertices(1,1),vertices(1,2),PaletteRed(colorIndex)*0.62,PaletteGreen(colorIndex)*0.58,PaletteBlue(colorIndex)*0.76,vertices(2,0),vertices(2,1),vertices(2,2),PaletteRed(colorIndex)*0.90,PaletteGreen(colorIndex)*0.86,PaletteBlue(colorIndex)*0.96
11830      NextPanel:
11840      NEXT panelId
11850      EXIT SUB

11860  ProjectPanelVertex:
11870      deltaX=worldX-CameraX : deltaY=worldY-CameraY : deltaZ=worldZ-CameraZ
11880      depth=deltaX*CameraForwardX+deltaY*CameraForwardY+deltaZ*CameraForwardZ
11890      pointX=0 : pointY=0 : pointDepth=0
11900      IF depth<0.35 THEN RETURN
11910      pointDepth=1/depth
11920      pointX=320+FocalLength*(deltaX*CameraRightX+deltaZ*CameraRightZ)*pointDepth
11930      pointY=ViewCenterY+FocalLength*(deltaX*CameraUpX+deltaY*CameraUpY+deltaZ*CameraUpZ)*pointDepth
11940      RETURN
11950  SUBEND

11960  DEF SUB DrawIntactLinks(depthBuffer,screenX,screenY,inverseDepth)
11970      LOCAL linkId, firstNode, secondNode, firstDepth, secondDepth, acrossX, acrossY, lengthSquared
11980      LOCAL inverseLength, firstWidth, secondWidth, colorIndex, strainGlow, red, green, blue
11990      LOCAL cornerAX, cornerAY, cornerBX, cornerBY, cornerCX, cornerCY, cornerDX, cornerDY

12000      FOR linkId=0 TO LinkCount-1
12010          IF LinkActive(linkId)=0 THEN GOTO NextIntactLink
12020          firstNode=LinkNodeA(linkId)  :  secondNode=LinkNodeB(linkId)  :  firstDepth=inverseDepth(firstNode)  :  secondDepth=inverseDepth(secondNode)  :  IF firstDepth<=0 OR secondDepth<=0 THEN GOTO NextIntactLink
12030          acrossX=screenX(secondNode)-screenX(firstNode)  :  acrossY=screenY(secondNode)-screenY(firstNode)  :  lengthSquared=acrossX*acrossX+acrossY*acrossY  :  IF lengthSquared<0.01 THEN GOTO NextIntactLink
12040          inverseLength=1/SQR(lengthSquared)  :  acrossX=acrossX*inverseLength  :  acrossY=acrossY*inverseLength  :  firstWidth=MAX(0.8,FocalLength*0.055*firstDepth)  :  secondWidth=MAX(0.8,FocalLength*0.055*secondDepth)
12050          colorIndex=NodeColor(secondNode)  :  strainGlow=MIN(1,ABS(LinkStrain(linkId))*20)  :  red=PaletteRed(colorIndex)*(1-strainGlow)+255*strainGlow  :  green=PaletteGreen(colorIndex)*(1-strainGlow)+145*strainGlow  :  blue=PaletteBlue(colorIndex)*(1-strainGlow)+85*strainGlow
12060          cornerAX=screenX(firstNode)-acrossY*firstWidth  :  cornerAY=screenY(firstNode)+acrossX*firstWidth  :  cornerBX=screenX(firstNode)+acrossY*firstWidth  :  cornerBY=screenY(firstNode)-acrossX*firstWidth
12070          cornerCX=screenX(secondNode)+acrossY*secondWidth  :  cornerCY=screenY(secondNode)-acrossX*secondWidth  :  cornerDX=screenX(secondNode)-acrossY*secondWidth  :  cornerDY=screenY(secondNode)+acrossX*secondWidth
12080          GTRIANGLE depthBuffer,cornerAX,cornerAY,firstDepth,red,green,blue,cornerBX,cornerBY,firstDepth,red*0.40,green*0.45,blue*0.55,cornerCX,cornerCY,secondDepth,red*0.40,green*0.45,blue*0.55
12090          GTRIANGLE depthBuffer,cornerAX,cornerAY,firstDepth,red,green,blue,cornerCX,cornerCY,secondDepth,red*0.40,green*0.45,blue*0.55,cornerDX,cornerDY,secondDepth,red,green,blue
12100      NextIntactLink:
12110      NEXT linkId
12120  SUBEND

12130  DEF SUB DrawJoints(depthBuffer,screenX,screenY,inverseDepth)
12140      LOCAL nodeId, depth, centerX, centerY, radius, colorIndex, angle, sinAngle
12150      LOCAL cosAngle, x1, y1, x2, y2, x3, y3, x4
12160      LOCAL y4

12170      FOR nodeId=0 TO NodeCount-1
12180          depth=inverseDepth(nodeId)  :  IF depth<=0 THEN GOTO NextJoint
12190          centerX=screenX(nodeId)  :  centerY=screenY(nodeId)  :  radius=MAX(1.1,NodeRadius(nodeId)*FocalLength*depth)  :  colorIndex=NodeColor(nodeId)
12200          IF nodeId=CoreNode THEN radius=radius*(1+0.045*SIN(AnimationTime*4)) : colorIndex=4
12210          IF WeakBearing(nodeId) AND BearingHit(nodeId)=0 THEN colorIndex=5 : radius=radius*1.55
12220          IF WeakBearing(nodeId) AND BearingHit(nodeId) THEN colorIndex=6
12230          angle=nodeId*1.71+SimulationTime*0.45  :  sinAngle=SIN(angle)  :  cosAngle=COS(angle)
12240          IF WeakBearing(nodeId) THEN sinAngle=0 : cosAngle=1
12250          x1=centerX+cosAngle*radius  :  y1=centerY+sinAngle*radius*0.6  :  x2=centerX-sinAngle*radius  :  y2=centerY+cosAngle*radius*1.6
12260          x3=centerX-cosAngle*radius  :  y3=centerY-sinAngle*radius*0.6  :  x4=centerX+sinAngle*radius  :  y4=centerY-cosAngle*radius*1.6
12270          GTRIANGLE depthBuffer,centerX,centerY,depth,255,255,245,x1,y1,depth,PaletteRed(colorIndex)*0.55,PaletteGreen(colorIndex)*0.65,PaletteBlue(colorIndex)*0.75,x2,y2,depth,PaletteRed(colorIndex),PaletteGreen(colorIndex),PaletteBlue(colorIndex)
12280          GTRIANGLE depthBuffer,centerX,centerY,depth,245,255,255,x2,y2,depth,PaletteRed(colorIndex),PaletteGreen(colorIndex),PaletteBlue(colorIndex),x3,y3,depth,PaletteRed(colorIndex)*0.35,PaletteGreen(colorIndex)*0.40,PaletteBlue(colorIndex)*0.58
12290          GTRIANGLE depthBuffer,centerX,centerY,depth,PaletteRed(colorIndex)*0.75,PaletteGreen(colorIndex)*0.85,PaletteBlue(colorIndex),x3,y3,depth,PaletteRed(colorIndex)*0.35,PaletteGreen(colorIndex)*0.40,PaletteBlue(colorIndex)*0.58,x4,y4,depth,PaletteRed(colorIndex)*0.28,PaletteGreen(colorIndex)*0.30,PaletteBlue(colorIndex)*0.43
12300          GTRIANGLE depthBuffer,centerX,centerY,depth,PaletteRed(colorIndex)*0.75,PaletteGreen(colorIndex)*0.85,PaletteBlue(colorIndex),x4,y4,depth,PaletteRed(colorIndex)*0.28,PaletteGreen(colorIndex)*0.30,PaletteBlue(colorIndex)*0.43,x1,y1,depth,PaletteRed(colorIndex)*0.55,PaletteGreen(colorIndex)*0.65,PaletteBlue(colorIndex)*0.75
12310      NextJoint:
12320      NEXT nodeId
12330  SUBEND

12340  DEF SUB DrawSparks(depthBuffer)
12350      LOCAL worldX, worldY, worldZ, pointX, pointY, pointDepth, radius, index
12360      LOCAL colorIndex, angle, cosAngle, sinAngle, brightness, deltaX, deltaY, deltaZ, depth

12370      IF ProjectileActive=0 THEN GOTO SparkPool
12380      worldX=ProjectileX : worldY=ProjectileY : worldZ=ProjectileZ : GOSUB ProjectSpark
12390      IF pointDepth>0 THEN radius=MAX(2,FocalLength*0.40*pointDepth) : GTRIANGLE depthBuffer,pointX,pointY+radius*1.4,pointDepth,255,255,255,pointX-radius,pointY-radius,pointDepth,88,211,255,pointX+radius,pointY-radius,pointDepth,218,248,255
12400      SparkPool:
12410      FOR index=0 TO 179
12420          IF SparkLifetime(index)<=0 THEN GOTO NextVisibleSpark
12430          worldX=SparkX(index) : worldY=SparkY(index) : worldZ=SparkZ(index) : GOSUB ProjectSpark
12440          IF pointDepth<=0 THEN GOTO NextVisibleSpark
12450          radius=MAX(0.7,SparkSize(index)*FocalLength*pointDepth)  :  colorIndex=SparkColor(index)  :  angle=AnimationTime*5+index*1.9  :  cosAngle=COS(angle)  :  sinAngle=SIN(angle)
12460          brightness=MIN(1,SparkLifetime(index)*2)
12470          GTRIANGLE depthBuffer,pointX+cosAngle*radius,pointY+sinAngle*radius,pointDepth,PaletteRed(colorIndex)*brightness,PaletteGreen(colorIndex)*brightness,PaletteBlue(colorIndex)*brightness,pointX-sinAngle*radius,pointY+cosAngle*radius,pointDepth,PaletteRed(colorIndex)*0.45,PaletteGreen(colorIndex)*0.55,PaletteBlue(colorIndex)*0.7,pointX-cosAngle*radius,pointY-sinAngle*radius,pointDepth,240*brightness,250*brightness,255*brightness
12480      NextVisibleSpark:
12490      NEXT index
12500      EXIT SUB

12510  ProjectSpark:
12520      deltaX=worldX-CameraX : deltaY=worldY-CameraY : deltaZ=worldZ-CameraZ
12530      depth=deltaX*CameraForwardX+deltaY*CameraForwardY+deltaZ*CameraForwardZ
12540      pointX=0 : pointY=0 : pointDepth=0
12550      IF depth<0.35 THEN RETURN
12560      pointDepth=1/depth
12570      pointX=320+FocalLength*(deltaX*CameraRightX+deltaZ*CameraRightZ)*pointDepth
12580      pointY=ViewCenterY+FocalLength*(deltaX*CameraUpX+deltaY*CameraUpY+deltaZ*CameraUpZ)*pointDepth
12590      RETURN
12600  SUBEND

12610  DEF SUB DrawSky()
12620      LOCAL strip, star

12630      FOR strip=0 TO 19
12640          FRECTANGLE 0,40+strip*20,639,59+strip*20,RGB(5+strip*0.17,8+strip*0.12,18+strip*0.45)
12650      NEXT strip
12660      FOR star=0 TO 89  :  PLOT StarX(star),StarY(star),StarColor(star)  :  NEXT star
12670  SUBEND

12680  DEF SUB DrawDais(depthBuffer)
12690      LOCAL worldX, worldY, worldZ, pointX, pointY, pointDepth, sector, angleA
12700      LOCAL angleB, centerX, centerY, centerDepth, rimAX, rimAY, rimADepth, rimBX
12710      LOCAL rimBY, rimBDepth, baseAX, baseAY, baseADepth, baseBX, baseBY, baseBDepth
12720      LOCAL shade, deltaX, deltaY, deltaZ, depth
12730      LOCAL aX, aY, aDepth, bX, bY, bDepth, cX, cY, cDepth, dX, dY, dDepth

12740      worldX=0 : worldY=0.25 : worldZ=0 : GOSUB ProjectDaisPoint
12750      centerX=pointX : centerY=pointY : centerDepth=pointDepth
12760      FOR sector=0 TO 15
12770          angleA=sector*PI/8  :  angleB=(sector+1)*PI/8
12780          worldX=COS(angleA)*8  :  worldY=0.25  :  worldZ=SIN(angleA)*8  :  GOSUB ProjectDaisPoint  :  rimAX=pointX  :  rimAY=pointY  :  rimADepth=pointDepth
12790          worldX=COS(angleB)*8  :  worldY=0.25  :  worldZ=SIN(angleB)*8  :  GOSUB ProjectDaisPoint  :  rimBX=pointX  :  rimBY=pointY  :  rimBDepth=pointDepth
12800          shade=0.85+0.15*(sector MOD 2)
12810          GTRIANGLE depthBuffer,centerX,centerY,centerDepth,26,37,55,rimAX,rimAY,rimADepth,20*shade,31*shade,47*shade,rimBX,rimBY,rimBDepth,23*shade,35*shade,52*shade
12820          worldX=COS(angleA)*8  :  worldY=-0.5  :  worldZ=SIN(angleA)*8  :  GOSUB ProjectDaisPoint  :  baseAX=pointX  :  baseAY=pointY  :  baseADepth=pointDepth
12830          worldX=COS(angleB)*8  :  worldY=-0.5  :  worldZ=SIN(angleB)*8  :  GOSUB ProjectDaisPoint  :  baseBX=pointX  :  baseBY=pointY  :  baseBDepth=pointDepth
12840          GTRIANGLE depthBuffer,rimAX,rimAY,rimADepth,33,61,76,baseAX,baseAY,baseADepth,8,14,28,baseBX,baseBY,baseBDepth,9,15,29
12850          GTRIANGLE depthBuffer,rimAX,rimAY,rimADepth,33,61,76,baseBX,baseBY,baseBDepth,9,15,29,rimBX,rimBY,rimBDepth,30,55,72
12860      NEXT sector
12870      GOSUB DrawInlay
12880      EXIT SUB

12890  DrawInlay:
12900      FOR sector=0 TO 31
12910          angleA=sector*PI/16  :  angleB=(sector+0.85)*PI/16
12920          worldX=COS(angleA)*7.3  :  worldY=0.265  :  worldZ=SIN(angleA)*7.3  :  GOSUB ProjectDaisPoint  :  aX=pointX  :  aY=pointY  :  aDepth=pointDepth
12930          worldX=COS(angleA)*7.4  :  worldY=0.265  :  worldZ=SIN(angleA)*7.4  :  GOSUB ProjectDaisPoint  :  bX=pointX  :  bY=pointY  :  bDepth=pointDepth
12940          worldX=COS(angleB)*7.4  :  worldY=0.265  :  worldZ=SIN(angleB)*7.4  :  GOSUB ProjectDaisPoint  :  cX=pointX  :  cY=pointY  :  cDepth=pointDepth
12950          worldX=COS(angleB)*7.3  :  worldY=0.265  :  worldZ=SIN(angleB)*7.3  :  GOSUB ProjectDaisPoint  :  dX=pointX  :  dY=pointY  :  dDepth=pointDepth
12960          GTRIANGLE depthBuffer,aX,aY,aDepth,37,86,110,bX,bY,bDepth,61,135,157,cX,cY,cDepth,61,135,157
12970          GTRIANGLE depthBuffer,aX,aY,aDepth,37,86,110,cX,cY,cDepth,61,135,157,dX,dY,dDepth,37,86,110
12980      NEXT sector
12990      RETURN

13000  ProjectDaisPoint:
13010      deltaX=worldX-CameraX : deltaY=worldY-CameraY : deltaZ=worldZ-CameraZ
13020      depth=deltaX*CameraForwardX+deltaY*CameraForwardY+deltaZ*CameraForwardZ
13030      pointX=0 : pointY=0 : pointDepth=0
13040      IF depth<0.35 THEN RETURN
13050      pointDepth=1/depth
13060      pointX=320+FocalLength*(deltaX*CameraRightX+deltaZ*CameraRightZ)*pointDepth
13070      pointY=ViewCenterY+FocalLength*(deltaX*CameraUpX+deltaY*CameraUpY+deltaZ*CameraUpZ)*pointDepth
13080      RETURN
13090  SUBEND

13100  REM === Scores, hints and controls ==========================================
13110  DEF SUB DrawHud()
13120      LOCAL bestText$, coreText$, progress, reticleColor
13130      FRECTANGLE 0,432,639,479,RGB(8,12,23) : FRECTANGLE 0,0,639,39,RGB(8,12,23)
13140      DRAW 0,431,639,431,RGB(42,111,132) : DRAW 0,40,639,40,RGB(42,111,132)
13150      bestText$=IIF(BestRescueShots>0,STR$(BestRescueShots),"--")
13160      SMALLFONT TRANSPARENT : INK RGB(177,237,247) : LOCATE ROUND(2*ScreenScaleX),ROUND(1*ScreenScaleY)
13170      GPRINT "AVL CRYSTAL SIEGE  ";USING "###.0";FramesPerSecond;" FPS  ORBS ";STR$(ShotsLeft);"/";STR$(MaxShots);"  RESCUE BEST ";bestText$
13180      INK RGB(133,159,188) : LOCATE ROUND(2*ScreenScaleX),ROUND(2*ScreenScaleY)
13190      IF RescueAwarded THEN GPRINT "CORE RESCUED - KEEP DEMOLISHING WITH YOUR REMAINING ORBS" ELSE GPRINT "FREE THE GOLD CORE - ORBIT TO FIND GREEN BEARINGS"
13200      INK RGB(152,184,202) : LOCATE ROUND(2*ScreenScaleX),ROUND(28*ScreenScaleY)
13210      GPRINT "CLICK/SPACE FIRE  ARROWS ORBIT  W/S ZOOM  T SLOW  P PAUSE  R REBUILD"
13220      LOCATE ROUND(2*ScreenScaleX),ROUND(29*ScreenScaleY) : GPRINT "D DEMO  Q SOLVER  F FPS  V SPARKS  B MUSIC  N MUTE  M MODE  ESC EXIT"

13230      coreText$=IIF(RescueAwarded,"FREE","HELD")
13240      progress=MIN(100,100*MIN(FallenFraction/0.68,HeightLossFraction/0.58))
13250      INK RGB(196,201,161) : LOCATE ROUND(2*ScreenScaleX),ROUND(25*ScreenScaleY)
13260      GPRINT "CORE ";coreText$;"  EXTRA DEMOLITION ";USING "###";progress;"%  BEARINGS ";STR$(BearingsBroken);"/";STR$(BearingCount);"  USED ";STR$(MaxShots-ShotsLeft)
13270      INK RGB(121,159,179) : LOCATE ROUND(2*ScreenScaleX),ROUND(26*ScreenScaleY)
13280      GPRINT NodeCount;" NODES ";LinkCount-BrokenLinkCount;" LINKS  Q";SolverIterations;"  PHYS ";USING "##.0";PhysicsMilliseconds;"ms DRAW ";USING "##.0";DrawingMilliseconds;"ms"
13290      CALL DrawDestructionScore
13300      IF FrameKey$="V" THEN
13310          ShowSparks=1-ShowSparks
13320          IF ShowSparks=0 THEN MAT SparkLifetime=0
13330      ENDIF

13340      reticleColor=IIF(ProjectileActive OR ShotsLeft=0,RGB(90,109,134),RGB(165,225,238))
13350      DRAW AimScreenX-9,AimScreenY,AimScreenX-4,AimScreenY,reticleColor
13360      DRAW AimScreenX+4,AimScreenY,AimScreenX+9,AimScreenY,reticleColor
13370      DRAW AimScreenX,AimScreenY-9,AimScreenX,AimScreenY-4,reticleColor
13380      DRAW AimScreenX,AimScreenY+4,AimScreenX,AimScreenY+9,reticleColor
13390      IF SlowMotion THEN INK RGB(119,221,243) : LOCATE ROUND(63*ScreenScaleX),ROUND(2*ScreenScaleY) : GPRINT "BULLET TIME"
13400      IF Paused THEN INK RGB(255,220,145) : LOCATE ROUND(WIDTH/16-9),ROUND(14*ScreenScaleY) : GPRINT "SIMULATION PAUSED"
13410      IF Muted THEN INK RGB(168,164,187) : LOCATE ROUND(65*ScreenScaleX),ROUND(26*ScreenScaleY) : GPRINT "MUTED"
13420      IF AutoSiege THEN INK RGB(250,208,143) : LOCATE ROUND(65*ScreenScaleX),ROUND(2*ScreenScaleY) : GPRINT "AUTO SIEGE"
13430      IF RescueAwarded AND AnimationTime<=RescueBannerUntil THEN CALL DrawRescueBanner(bestText$) : EXIT SUB
13440      IF ShotsLeft=0 AND ProjectileActive=0 THEN INK RGB(244,184,142) : LOCATE ROUND(WIDTH/16-32),ROUND(24*ScreenScaleY) : GPRINT "NO ORBS LEFT - LET THE PIECES SETTLE OR R TO REBUILD"
13450  SUBEND

13460  DEF SUB DrawDestructionScore()
13470      SMALLFONT TRANSPARENT : INK RGB(143,237,183) : LOCATE ROUND(2*ScreenScaleX),ROUND(27*ScreenScaleY)
13480      GPRINT "DESTRUCTION ";STR$(DestructionScore);" PTS  RECORD ";STR$(BestDestructionScore)
13490  SUBEND

13500  DEF SUB DrawRescueBanner(bestText$)
13510      FRECTANGLE 123,365,517,432,RGB(10,24,34) : RECTANGLE 123,365,517,432,RGB(245,205,108)
13520      BIGFONT TRANSPARENT : INK RGB(255,224,142) : LOCATE ROUND(WIDTH/32-7),ROUND(4*ScreenScaleY) : GPRINT "GOLD CORE FREE"
13530      SMALLFONT TRANSPARENT : INK RGB(184,229,237) : LOCATE ROUND(WIDTH/16-20),ROUND(6*ScreenScaleY)
13540      GPRINT "IN ";STR$(RescueShots);" SHOTS | BEST ";bestText$;" | KEEP DEMOLISHING"
13550      CALL DrawDestructionScore
13560  SUBEND
13570  REM === Solid rendering of detached beams =====================================
13580  DEF SUB RenderBeamDebris(DepthBuffer)
13590      LOCAL beamId, corner, endpoint, vertex, face, colorIndex, cornerA, cornerB, cornerC, cornerD
13600      LOCAL axisX, axisY, axisZ, axisLength, sideX, sideY, sideZ, sideLength
13610      LOCAL crossX, crossY, crossZ, offsetX, offsetY, offsetZ
13620      LOCAL centerX, centerY, centerZ, normalX, normalY, normalZ
13630      LOCAL faceX, faceY, faceZ, normalLength, brightness, red, green, blue
13640      LOCAL worldX, worldY, worldZ, deltaX, deltaY, deltaZ, depth, inverseDepth
13650      LOCAL vertices(7,2)

13660      FOR beamId = 0 TO LinkCount - 1
13670          IF BeamState(beamId) = 0 THEN GOTO NextLooseBeam
13680          axisX = BeamX(beamId,1) - BeamX(beamId,0)
13690          axisY = BeamY(beamId,1) - BeamY(beamId,0)
13700          axisZ = BeamZ(beamId,1) - BeamZ(beamId,0)
13710          axisLength = SQR(axisX * axisX + axisY * axisY + axisZ * axisZ)
13720          IF axisLength < 0.00001 THEN GOTO NextLooseBeam
13730          axisX = axisX / axisLength : axisY = axisY / axisLength : axisZ = axisZ / axisLength

13740          sideX = -axisZ : sideY = 0 : sideZ = axisX
13750          IF ABS(axisY) > 0.9 THEN sideX = 0 : sideY = axisZ : sideZ = -axisY
13760          sideLength = SQR(sideX * sideX + sideY * sideY + sideZ * sideZ)
13770          sideX = sideX / sideLength : sideY = sideY / sideLength : sideZ = sideZ / sideLength
13780          crossX = axisY * sideZ - axisZ * sideY
13790          crossY = axisZ * sideX - axisX * sideZ
13800          crossZ = axisX * sideY - axisY * sideX

13810          REM Four corners on each end make a diamond-section prism.
13820          FOR corner = 0 TO 3
13830              IF corner = 0 THEN
13840                  offsetX = sideX : offsetY = sideY : offsetZ = sideZ
13850              ELSEIF corner = 1 THEN
13860                  offsetX = crossX : offsetY = crossY : offsetZ = crossZ
13870              ELSEIF corner = 2 THEN
13880                  offsetX = -sideX : offsetY = -sideY : offsetZ = -sideZ
13890              ELSE
13900                  offsetX = -crossX : offsetY = -crossY : offsetZ = -crossZ
13910              ENDIF
13920              FOR endpoint = 0 TO 1
13930                  worldX = BeamX(beamId,endpoint) + offsetX * BeamRadius
13940                  worldY = BeamY(beamId,endpoint) + offsetY * BeamRadius
13950                  worldZ = BeamZ(beamId,endpoint) + offsetZ * BeamRadius
13960                  vertex = corner + endpoint * 4 : GOSUB ProjectCorner
13970              NEXT endpoint
13980          NEXT corner
13990          centerX = (BeamX(beamId,0) + BeamX(beamId,1)) * 0.5
14000          centerY = (BeamY(beamId,0) + BeamY(beamId,1)) * 0.5
14010          centerZ = (BeamZ(beamId,0) + BeamZ(beamId,1)) * 0.5
14020          colorIndex = BeamColor(beamId)

14030          FOR face = 0 TO 5
14040              IF face < 4 THEN
14050                  normalX = sideX + crossX : normalY = sideY + crossY : normalZ = sideZ + crossZ
14060                  IF face = 1 THEN normalX = crossX - sideX : normalY = crossY - sideY : normalZ = crossZ - sideZ
14070                  IF face = 2 THEN normalX = -sideX - crossX : normalY = -sideY - crossY : normalZ = -sideZ - crossZ
14080                  IF face = 3 THEN normalX = sideX - crossX : normalY = sideY - crossY : normalZ = sideZ - crossZ
14090                  faceX = centerX + normalX * BeamRadius * 0.5
14100                  faceY = centerY + normalY * BeamRadius * 0.5
14110                  faceZ = centerZ + normalZ * BeamRadius * 0.5
14120                  cornerA = face : cornerB = (face + 1) MOD 4 : cornerD = face + 4 : cornerC = cornerB + 4
14130              ELSEIF face = 4 THEN
14140                  cornerA = 0 : cornerB = 3 : cornerC = 2 : cornerD = 1
14150                  normalX = -axisX : normalY = -axisY : normalZ = -axisZ
14160                  faceX = BeamX(beamId,0) : faceY = BeamY(beamId,0) : faceZ = BeamZ(beamId,0)
14170              ELSE
14180                  cornerA = 4 : cornerB = 5 : cornerC = 6 : cornerD = 7
14190                  normalX = axisX : normalY = axisY : normalZ = axisZ
14200                  faceX = BeamX(beamId,1) : faceY = BeamY(beamId,1) : faceZ = BeamZ(beamId,1)
14210              ENDIF
14220              IF (CameraX - faceX) * normalX + (CameraY - faceY) * normalY + (CameraZ - faceZ) * normalZ > 0 THEN
14230                  normalLength = SQR(normalX * normalX + normalY * normalY + normalZ * normalZ)
14240                  brightness = 0.38 + 0.62 * MAX(0, MIN(1, (normalX * 0.35 + normalY * 0.84 + normalZ * 0.42) / normalLength))
14250                  red = PaletteRed(colorIndex) * brightness : green = PaletteGreen(colorIndex) * brightness : blue = PaletteBlue(colorIndex) * brightness
14260                  GOSUB DrawFace
14270              ENDIF
14280          NEXT face
14290  NextLooseBeam:
14300      NEXT beamId
14310      EXIT SUB

14320  ProjectCorner:
14330      REM These hot helpers share only this rendering procedure's local scratch.
14340      deltaX = worldX - CameraX : deltaY = worldY - CameraY : deltaZ = worldZ - CameraZ
14350      depth = deltaX * CameraForwardX + deltaY * CameraForwardY + deltaZ * CameraForwardZ
14360      vertices(vertex,0) = 0 : vertices(vertex,1) = 0 : vertices(vertex,2) = 0
14370      IF depth < 0.35 THEN RETURN
14380      inverseDepth = 1 / depth
14390      vertices(vertex,0) = 320 + FocalLength * (deltaX * CameraRightX + deltaZ * CameraRightZ) * inverseDepth
14400      vertices(vertex,1) = ViewCenterY + FocalLength * (deltaX * CameraUpX + deltaY * CameraUpY + deltaZ * CameraUpZ) * inverseDepth
14410      vertices(vertex,2) = inverseDepth
14420      RETURN

14430  DrawFace:
14440      IF MIN(vertices(cornerA,2), MIN(vertices(cornerB,2), MIN(vertices(cornerC,2), vertices(cornerD,2)))) <= 0 THEN RETURN
14450      GTRIANGLE DepthBuffer, vertices(cornerA,0),vertices(cornerA,1),vertices(cornerA,2),red,green,blue, vertices(cornerB,0),vertices(cornerB,1),vertices(cornerB,2),red*0.82,green*0.86,blue*0.90, vertices(cornerC,0),vertices(cornerC,1),vertices(cornerC,2),red*0.82,green*0.86,blue*0.90
14460      GTRIANGLE DepthBuffer, vertices(cornerA,0),vertices(cornerA,1),vertices(cornerA,2),red,green,blue, vertices(cornerC,0),vertices(cornerC,1),vertices(cornerC,2),red*0.82,green*0.86,blue*0.90, vertices(cornerD,0),vertices(cornerD,1),vertices(cornerD,2),red,green,blue
14470      RETURN
14480  SUBEND

89990  REM === Rest nodes: numeric RESTORE anchor 90000 ===
90000  DATA -4,0.25,-2.6,0,1,0.22,1
90010  DATA -4,2.5,-2.6,1,1,0.18,1
90020  DATA -2,0.25,-2.6,0,1,0.22,1
90030  DATA -2,2.5,-2.6,1,1,0.18,1
90040  DATA -2,5.2,-2.6,1,1,0.18,2
90050  DATA -1.1,7,-2.6,1,1,0.18,2
90060  DATA 0,8,-2.6,1,1,0.18,2
90070  DATA 1.1,7,-2.6,1,1,0.18,2
90080  DATA 2,5.2,-2.6,1,1,0.18,3
90090  DATA 2,2.5,-2.6,1,1,0.18,3
90100  DATA 2,0.25,-2.6,0,1,0.22,3
90110  DATA 4,2.5,-2.6,1,1,0.18,3
90120  DATA 4,0.25,-2.6,0,1,0.22,1
90130  DATA -4,0.25,-1.3,0,1,0.22,2
90140  DATA -4,2.5,-1.3,1,1,0.18,2
90150  DATA -2,0.25,-1.3,0,1,0.22,2
90160  DATA -2,2.5,-1.3,1,1,0.18,2
90170  DATA -2,5.2,-1.3,1,1,0.18,3
90180  DATA -1.1,7,-1.3,1,1,0.18,3
90190  DATA 0,8,-1.3,1,1,0.18,3
90200  DATA 1.1,7,-1.3,1,1,0.18,3
90210  DATA 2,5.2,-1.3,1,1,0.18,1
90220  DATA 2,2.5,-1.3,1,1,0.18,1
90230  DATA 2,0.25,-1.3,0,1,0.22,1
90240  DATA 4,2.5,-1.3,1,1,0.18,1
90250  DATA 4,0.25,-1.3,0,1,0.22,2
90260  DATA -4,0.25,0,0,1,0.22,3
90270  DATA -4,2.5,0,1,1,0.18,3
90280  DATA -2,0.25,0,0,1,0.22,3
90290  DATA -2,2.5,0,1,1,0.18,3
90300  DATA -2,5.2,0,1,1,0.18,1
90310  DATA -1.1,7,0,1,1,0.18,1
90320  DATA 0,8,0,1,1,0.18,1
90330  DATA 1.1,7,0,1,1,0.18,1
90340  DATA 2,5.2,0,1,1,0.18,2
90350  DATA 2,2.5,0,1,1,0.18,2
90360  DATA 2,0.25,0,0,1,0.22,2
90370  DATA 4,2.5,0,1,1,0.18,2
90380  DATA 4,0.25,0,0,1,0.22,3
90390  DATA -4,0.25,1.3,0,1,0.22,1
90400  DATA -4,2.5,1.3,1,1,0.18,1
90410  DATA -2,0.25,1.3,0,1,0.22,1
90420  DATA -2,2.5,1.3,1,1,0.18,1
90430  DATA -2,5.2,1.3,1,1,0.18,2
90440  DATA -1.1,7,1.3,1,1,0.18,2
90450  DATA 0,8,1.3,1,1,0.18,2
90460  DATA 1.1,7,1.3,1,1,0.18,2
90470  DATA 2,5.2,1.3,1,1,0.18,3
90480  DATA 2,2.5,1.3,1,1,0.18,3
90490  DATA 2,0.25,1.3,0,1,0.22,3
90500  DATA 4,2.5,1.3,1,1,0.18,3
90510  DATA 4,0.25,1.3,0,1,0.22,1
90520  DATA -4,0.25,2.6,0,1,0.22,2
90530  DATA -4,2.5,2.6,1,1,0.18,2
90540  DATA -2,0.25,2.6,0,1,0.22,2
90550  DATA -2,2.5,2.6,1,1,0.18,2
90560  DATA -2,5.2,2.6,1,1,0.18,3
90570  DATA -1.1,7,2.6,1,1,0.18,3
90580  DATA 0,8,2.6,1,1,0.18,3
90590  DATA 1.1,7,2.6,1,1,0.18,3
90600  DATA 2,5.2,2.6,1,1,0.18,1
90610  DATA 2,2.5,2.6,1,1,0.18,1
90620  DATA 2,0.25,2.6,0,1,0.22,1
90630  DATA 4,2.5,2.6,1,1,0.18,1
90640  DATA 4,0.25,2.6,0,1,0.22,2
90650  DATA -2.38,8.05,-2.98,1,1,0.17,2
90660  DATA -1.62,8.05,-2.98,1,1,0.17,2
90670  DATA -1.62,8.05,-2.22,1,1,0.17,2
90680  DATA -2.38,8.05,-2.22,1,1,0.17,2
90690  DATA -2,10.2,-2.6,1,1,0.27,2
90700  DATA 1.62,8.05,-2.98,1,1,0.17,3
90710  DATA 2.38,8.05,-2.98,1,1,0.17,3
90720  DATA 2.38,8.05,-2.22,1,1,0.17,3
90730  DATA 1.62,8.05,-2.22,1,1,0.17,3
90740  DATA 2,10.2,-2.6,1,1,0.27,3
90750  DATA 0,9.5,2.6,1,1,0.23,3
90760  DATA 0,6.6,0,1,2,0.46,4

90990  REM === Structural links: numeric RESTORE anchor 91000 ===
91000  DATA 0,1,1,2,1,3,1,4,1,13,1,14
91010  DATA 1,16,3,4,3,5,3,6,3,9,3,14
91020  DATA 3,16,3,17,4,5,4,6,4,8,4,16
91030  DATA 4,17,4,18,4,21,4,65,4,66,4,67
91040  DATA 4,68,5,6,5,17,5,18,5,19,6,7
91050  DATA 6,8,6,9,6,18,6,19,6,20,7,8
91060  DATA 7,9,7,19,7,20,7,21,8,9,8,11
91070  DATA 8,17,8,20,8,21,8,22,8,70,8,71
91080  DATA 8,72,8,73,9,11,9,21,9,22,9,24
91090  DATA 10,11,11,12,11,22,11,24,11,25,13,27
91100  DATA 14,16,14,17,14,27,14,29,16,17,16,18
91110  DATA 16,19,16,22,16,27,16,29,16,30,17,18
91120  DATA 17,19,17,21,17,29,17,30,17,31,17,34
91130  DATA 17,65,17,66,18,19,18,30,18,31,18,32
91140  DATA 18,67,18,68,19,20,19,21,19,22,19,31
91150  DATA 19,32,19,33,19,76,20,21,20,22,20,32
91160  DATA 20,33,20,34,20,72,20,73,21,22,21,24
91170  DATA 21,30,21,33,21,34,21,35,21,70,21,71
91180  DATA 22,24,22,34,22,35,22,37,24,35,24,37
91190  DATA 25,37,26,27,27,28,27,29,27,30,27,39
91200  DATA 27,40,27,42,29,30,29,31,29,32,29,35
91210  DATA 29,40,29,42,29,43,30,31,30,32,30,34
91220  DATA 30,42,30,43,30,44,30,47,30,76,31,32
91230  DATA 31,43,31,44,31,45,31,76,32,33,32,34
91240  DATA 32,35,32,44,32,45,32,46,32,76,33,34
91250  DATA 33,35,33,45,33,46,33,47,33,76,34,35
91260  DATA 34,37,34,43,34,46,34,47,34,48,34,76
91270  DATA 35,37,35,47,35,48,35,50,36,37,37,38
91280  DATA 37,48,37,50,37,51,39,53,40,42,40,43
91290  DATA 40,53,40,55,42,43,42,44,42,45,42,48
91300  DATA 42,53,42,55,42,56,43,44,43,45,43,47
91310  DATA 43,55,43,56,43,57,43,60,44,45,44,56
91320  DATA 44,57,44,58,45,46,45,47,45,48,45,57
91330  DATA 45,58,45,59,45,76,46,47,46,48,46,58
91340  DATA 46,59,46,60,47,48,47,50,47,56,47,59
91350  DATA 47,60,47,61,48,50,48,60,48,61,48,63
91360  DATA 50,61,50,63,51,63,52,53,53,54,53,55
91370  DATA 53,56,55,56,55,57,55,58,55,61,56,57
91380  DATA 56,58,56,60,56,75,57,58,57,75,58,59
91390  DATA 58,60,58,61,58,75,59,60,59,61,59,75
91400  DATA 60,61,60,63,60,75,61,63,62,63,63,64
91410  DATA 65,66,65,67,65,68,65,69,66,67,66,68
91420  DATA 66,69,67,68,67,69,68,69,70,71,70,72
91430  DATA 70,73,70,74,71,72,71,73,71,74,72,73
91440  DATA 72,74,73,74

91990  REM === Stained-glass panels: numeric RESTORE anchor 92000 ===
92000  DATA 3,4,17,7,18,13,3,17,16,13,64,12,4,5,18,14,27,19
92010  DATA 4,18,17,19,71,18,5,6,19,25,33,28,5,19,18,28,80,27
92020  DATA 6,7,20,29,38,34,6,20,19,34,86,33,7,8,21,35,44,39
92030  DATA 7,21,20,39,93,38,8,9,22,40,52,45,8,22,21,45,100,44
92040  DATA 16,17,30,64,75,70,16,30,29,70,122,69,17,18,31,71,82,76
92050  DATA 17,31,30,76,129,75,18,19,32,80,90,83,18,32,31,83,137,82
92060  DATA 19,20,33,86,96,91,19,33,32,91,142,90,20,21,34,93,104,97
92070  DATA 20,34,33,97,149,96,21,22,35,100,110,105,21,35,34,105,155,104
92080  DATA 29,30,43,122,133,128,29,43,42,128,176,127,30,31,44,129,139,134
92090  DATA 30,44,43,134,183,133,31,32,45,137,146,140,31,45,44,140,190,139
92100  DATA 32,33,46,142,152,147,32,46,45,147,194,146,33,34,47,149,159,153
92110  DATA 33,47,46,153,201,152,34,35,48,155,164,160,34,48,47,160,206,159
92120  DATA 42,43,56,176,187,182,42,56,55,182,223,181,43,44,57,183,192,188
92130  DATA 43,57,56,188,227,187,44,45,58,190,198,193,44,58,57,193,231,192
92140  DATA 45,46,59,194,204,199,45,59,58,199,233,198,46,47,60,201,210,205
92150  DATA 46,60,59,205,237,204,47,48,61,206,214,211,47,61,60,211,240,210
92160  DATA 4,65,66,21,246,22,69,66,65,252,246,249,4,66,67,22,250,23
92170  DATA 69,67,66,254,250,252,4,67,68,23,253,24,69,68,67,255,253,254
92180  DATA 4,68,65,24,248,21,69,65,68,249,248,255,8,70,71,46,256,47
92190  DATA 74,71,70,262,256,259,8,71,72,47,260,48,74,72,71,264,260,262
92200  DATA 8,72,73,48,263,49,74,73,72,265,263,264,8,73,70,49,258,46
92210  DATA 74,70,73,259,258,265

92990  REM === Automatic siege route: numeric RESTORE anchor 93000 ===
93000  DATA 1,-17,-12,-2.52692970166763,0.124303819963577,20.970455407549
93010  DATA 11,17,-12,-0.614662951922166,0.124303819963577,20.970455407549
93020  DATA 53,-17,12,2.52692970166763,0.124303819963577,20.970455407549
93030  DATA 63,17,12,0.614662951922166,0.124303819963577,20.970455407549
93040  DATA 27,-20,0,3.14159265358979,0.129275004048143,20.1682919455268
93050  DATA 37,20,0,0,0.129275004048143,20.1682919455268
