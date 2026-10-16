10 ' Dustline - a dirt-track time trial with scrolling prerendered scenery.
20 ' Arrows/WASD: drive. Space: handbrake. G: ghost. P: pause. R: restart. M: sound. D: demo.
30 ' World: 10 pixels/metre; oblique orthographic projection, Y compressed to 65%.
40 ' Physics, banked terrain contacts, mutable pixel mud, timing and audio are BASIC.
50 SCREEN
60 MODE 800
70 RAD
80 MAT BASE 0
90 PAPER RGB(22,32,31)
100 CLG OFFSCREEN
110 SMALLFONT TRANSPARENT
120 COLMODE 0
130 H=1/120
140 Inertia=205
150 ZProj=SQR(1-0.65^2)
160 CarScale=1.28 : Axle=13*CarScale : ContactArm=11*CarScale : WallContact=104-14*CarScale
200 RANDOMIZE 731
210 Muted=0 : Demo=0 : Paused=0 : Phase=0 : PauseTotal=0
260 SessionBest=0 : GhostShow=1 : GhostN=0 : GhostBank=1 : RecBank=0
310 MudClock=0 : MudSeed=731
330 SectorGreen=RGB(48,255,64) : SectorRed=RGB(255,55,40)
350 ' Shared state: car pose, terrain query results, lap/session state and bounded buffers.
360 CALL loadTrack
370 CALL resetAttempt
380 ' Geometry and buffers are ready; optional audio and saved record follow.
390 ON ERROR GOTO 1470
400 CALL loadAudio
410 CALL loadPersonalBest
420 Fps=0 : FpsCount=0 : FpsT=TIME
450 PrevT=TIME : Accum=0
470 Cx=MAX(0,MIN(MapW-800,Px-400)) : Cy=MAX(0,MIN(MapH-600,Py*Iso-300))
490 ' Render at 60 Hz; integrate the car at a fixed 120 Hz.
500 WHILE 1
510   Now=TIME
520   Elapsed=MAX(0,MIN(0.12,Now-PrevT))
530   PrevT=Now
540   K$=UPPER$(INKEY$)
550   IF K$=CHR$(27) OR KEYDOWN(27) THEN EXIT WHILE
580   CALL handleCommands
590   Throttle=0 : Brake=0 : Handbrake=0 : Turn=0
630   IF Phase=2 AND Paused=0 THEN CALL readControls
660   IF Phase=2 AND Paused=0 THEN
670     Accum=Accum+Elapsed : RenderDt=Elapsed
690     WHILE Accum>=H
700       RaceNow=TIME-PauseTotal
710       IF Demo THEN CALL driveDemo
740       CALL stepCar
750       CALL updateMud
760       CALL updateDust
770       Accum=Accum-H
780       CALL recordGhost
790     WEND
800   ELSE
810     Accum=0
820   ENDIF
830   RaceNow=TIME-PauseTotal
840   IF Started AND Paused=0 THEN LapTime=MAX(0,RaceNow-LapStart)
870   CamDt=MAX(0.001,Elapsed)
880   IF Paused THEN CamDt=0
910   Tx=MAX(0,MIN(MapW-800,Px+Vx*0.28-400)) : Ty=MAX(0,MIN(MapH-600,Py*Iso+Vy*Iso*0.28-330))
930   Cx=Cx+(Tx-Cx)*MIN(1,5*CamDt) : Cy=Cy+(Ty-Cy)*MIN(1,5*CamDt)
950   CALL drawWorld
960   CALL drawHud
970   IF Phase=0 THEN CALL drawTitle
1000   IF Paused THEN CALL drawPause
1030   IF Phase=1 THEN CALL drawCountdown
1060   CALL updateAudio
1070   FRAME 60
1080   FpsCount=FpsCount+1
1090   IF TIME-FpsT>=0.5 THEN
1100     Fps=FpsCount/(TIME-FpsT)
1110     FpsCount=0 : FpsT=TIME
1130   ENDIF
1140 WEND
1150 AUDIO STOP : SCREEN CLOSE
1170 END
1180 ' Each record procedure handles its own I/O errors and returns normally to its caller.
1190 DEF SUB loadPersonalBest
1195   LOCAL Magic$
1200   Best=0 : MAT Bests=0
1210   ON ERROR GOTO 1360
1240   OPEN "g-dustline-best.csv" FOR INPUT AS #1
1250   IF EOF(1) THEN CLOSE #1:EXIT SUB
1290   INPUT #1,Magic$,Best,Bests(1),Bests(2),Bests(3)
1300   CLOSE #1
1310   IF Magic$<>"DUSTLINE/3" OR Best<5 OR Best>86400 THEN Best=0:MAT Bests=0
1350   IF Bests(1)<0 OR Bests(2)<Bests(1) OR Bests(3)<Bests(2) THEN Best=0:MAT Bests=0
1355   EXIT SUB
1360   CLOSE
1370   Best=0 : MAT Bests=0
1380   RESUME 1390
1390   EXIT SUB
1400 SUBEND
1410 DEF SUB savePersonalBest
1415   ON ERROR GOTO 1445
1420   OPEN "g-dustline-best.csv" FOR OUTPUT AS #1
1430   WRITE #1,"DUSTLINE/3",Best,Bests(1),Bests(2),Bests(3)
1440   CLOSE #1
1442   EXIT SUB
1445   CLOSE
1450   Msg$="RECORD KEPT IN MEMORY - SAVE FAILED" : MsgT=TIME+4
1455   RESUME 1458
1458   EXIT SUB
1460 SUBEND
1470 ' The global fallback stops the game on errors not handled by a record procedure.
1600 PRINT "ERROR ";ERR;" AT LINE ";ERL
1610 AUDIO STOP
1620 SCREEN CLOSE
1630 END
1640 ' ----------------------------------------------------------------------
1650 DEF SUB loadTrack
1660   LOCAL I
1670   ' Load one shared track geometry for art, contacts, ramp and ordered gates.
1680   OPEN "assets/dustline/track.csv" FOR INPUT AS #1
1690   INPUT #1,Np,MapW,WorldH,Iso,GridW,GridH,GridStep,NGates,TrackLength
1700   MapH=INT(WorldH*Iso+0.5)
1710   INPUT #1,Rx,Ry,RTx,RTy,Rl,Rh,Rw
1720   DIM TrX(Np-1),TrY(Np-1),TrDx(Np-1),TrDy(Np-1),TrL(Np-1),TrZ(Np-1),TrBank(Np-1)
1730   DIM GateX(NGates-1),GateY(NGates-1),GateDx(NGates-1),GateDy(NGates-1),GateI(NGates-1)
1740   DIM Grid$(GridH-1),Splits(3),Bests(3)
1750   ' Two bounded pose buffers swap on a session record; no physics is replayed.
1760   MaxGhost=18000
1770   DIM Gt(1,MaxGhost-1),Gx(1,MaxGhost-1),Gy(1,MaxGhost-1),Gz(1,MaxGhost-1),Ga(1,MaxGhost-1),Gf(1,MaxGhost-1),Gp(1,MaxGhost-1)
1780   DIM SectorBest(3),MMx(Np),MMy(Np),MMBase(3),MiniView$(6)
1790   FOR I=0 TO Np-1
1800     INPUT #1,TrX(I),TrY(I),TrDx(I),TrDy(I),TrL(I),TrZ(I),TrBank(I)
1810   NEXT I
1820   FOR I=0 TO NGates-1
1830     INPUT #1,GateX(I),GateY(I),GateDx(I),GateDy(I),GateI(I)
1840   NEXT I
1850   CLOSE #1
1860   OPEN "assets/dustline/lookup.txt" FOR INPUT AS #1
1870   FOR I=0 TO GridH-1
1880     LINE INPUT #1,Grid$(I)
1890   NEXT I
1900   CLOSE #1
1910   ' Mutable soil: one depth/direction/mixing state per occupied six-pixel patch.
1920   MaxMark=30000 : MudStep=6
1940   MudW=INT(MapW/MudStep)+1 : MudH=INT(MapH/MudStep)+1
1960   TileCols=INT(MapW/256)+1 : TileRows=INT(MapH/256)+1
1980   DIM Head(TileCols*TileRows),MNext(MaxMark),MPrev(MaxMark),MCell(MaxMark)
1990   DIM M1x(MaxMark),M1y(MaxMark),M2x(MaxMark),M2y(MaxMark),M3x(MaxMark),M3y(MaxMark),MGx(MaxMark),MGy(MaxMark)
2000   DIM MXMin(MaxMark),MXMax(MaxMark),MYMin(MaxMark),MYMax(MaxMark),MCol(MaxMark),MEdge(MaxMark),MBase(MaxMark)
2010   DIM MGrid(MudW-1,MudH-1),MDepth(MaxMark),MDirX(MaxMark),MDirY(MaxMark),MChurn(MaxMark),MStamp(MaxMark)
2020   DIM MTouch(MaxMark),MVar(MaxMark),MFade(MaxMark)
2030   MaxDust=60
2040   DIM DustX(MaxDust-1),DustY(MaxDust-1),DustZ(MaxDust-1),DustVx(MaxDust-1),DustVy(MaxDust-1),DustL(MaxDust-1),DustAge(MaxDust-1)
2050   INK RGB(230,205,155) : MOVE 34,557 : LABEL "DUSTLINE  /  LOADING LA CANTERA..."
2080   FRAME
2090   BLOAD "assets/dustline/circuit.png",Track$
2100   BLOAD "assets/dustline/car.png",Car$
2110   BLOAD "assets/dustline/shadow.png",Shadow$
2120   BLOAD "assets/dustline/minimap.png",MiniMap$
2130   CALL buildMinimap(MMx,MMy,MMBase,MiniView$)
2140 SUBEND
2150 ' ----------------------------------------------------------------------
2160 DEF SUB loadAudio
2170   ' Optional audio: absence of a device does not change driving or timing.
2180   AUDIO LOAD 1,"assets/dustline/engine.wav"
2190   AUDIO LOAD 2,"assets/dustline/gravel.wav"
2200   AUDIO LOAD 3,"assets/dustline/impact.wav"
2210   AUDIO LOAD 4,"assets/dustline/lap.wav"
2220   AudioReady=1
2230   CALL resetAudio
2240 SUBEND
2250 ' ----------------------------------------------------------------------
2260 DEF SUB resetAttempt
2270   ' Restart only this attempt; tyre marks, saved record and session best survive.
2280   Px=TrX(0)-TrDx(0)*60 : Py=TrY(0)-TrDy(0)*60
2300   Vx=0 : Vy=0 : Yaw=0 : Steer=0
2340   Ang=ATN(TrDy(0)/TrDx(0))
2350   Z=0 : Vz=0 : Air=0 : FloorZ=0
2390   Speed=0 : Vf=0 : Vl=0 : Slip=0
2430   Qx=Px : Qy=Py
2450   CALL sampleTerrain
2460   Z=FloorZ
2470   SlipR=0
2480   Started=0 : NextGate=0
2500   LapTime=0 : LapStart=0 : WrongTime=0
2530   Invalid=0
2540   MAT Splits=0
2550   Paused=0 : Accum=0
2570   MarkValid=0 : MarkT=0 : DustT=0
2600   HitCool=0 : Rumble=0 : CamDt=0
2630   Cx=MAX(0,MIN(MapW-800,Px-400)) : Cy=MAX(0,MIN(MapH-600,Py*Iso-330))
2650   Msg$="" : MsgT=0
2670   LastLap=0 : LastDelta=0 : Laps=0
2700   Collisions=0 : Jumps=0 : MaxHeight=0
2730   RecN=0 : RecFull=0 : GhostPlay=0
2760   SectorDone=0 : MiniView=0 : SplitDelta=0
2790   MAT DustL=0
2800   IF AudioReady THEN CALL resetAudio
2830 SUBEND
2840 ' ----------------------------------------------------------------------
2850 DEF SUB resetAudio
2860   AUDIO STOP
2862   AudioGear=1 : AudioShift=0
2863   IF Speed*0.36>=45 THEN AudioGear=2
2864   IF Speed*0.36>=85 THEN AudioGear=3
2865   IF Speed*0.36>=130 THEN AudioGear=4
2866   IF Speed*0.36>=180 THEN AudioGear=5
2867   IF Vf<-8 AND Speed<120 THEN AudioGear=1
2870   IF Muted OR AudioReady=0 THEN EXIT SUB
2900   AUDIO PLAY 1,1,1,0.12,0,0.7
2910   AUDIO PLAY 2,2,1,0,0,1
2920 SUBEND
2930 ' ----------------------------------------------------------------------
2940 DEF SUB updateAudio
2950   LOCAL Rpm, EngineVol, DriftVol, Kmh, NextGear, GearTop
2955   ' These five gears affect sound only; all driving state is read unchanged.
2960   IF Muted OR AudioReady=0 THEN EXIT SUB
2990   IF Paused OR Phase<>2 THEN
2995     AUDIO RATE 1,0.72,60
3000     AUDIO VOLUME 1,0.08,50 : AUDIO VOLUME 2,0,10
3020     EXIT SUB
3030   ENDIF
3040   AudioShift=MAX(0,AudioShift-Elapsed)
3042   Kmh=Speed*0.36
3044   EngineVol=0.11+Throttle*0.12
3046   IF Vf<-8 AND Speed<120 AND Air=0 THEN
3048     AudioGear=1 : AudioShift=0
3050     Rpm=0.72+MIN(2.1,ABS(Vf)/65)+Brake*0.12
3052     EngineVol=0.11+Brake*0.12
3054   ELSE
3056     IF Kmh<5 AND Air=0 THEN AudioGear=1:AudioShift=0
3058     NextGear=AudioGear
3060     ' Separate up/down thresholds and a short cooldown prevent gear hunting.
3062     IF AudioShift=0 AND Air=0 THEN
3064       IF AudioGear=1 AND Kmh>=45 THEN NextGear=2
3066       IF AudioGear=2 AND Kmh>=85 THEN NextGear=3
3068       IF AudioGear=3 AND Kmh>=130 THEN NextGear=4
3070       IF AudioGear=4 AND Kmh>=180 THEN NextGear=5
3072       IF AudioGear=2 AND Kmh<=35 THEN NextGear=1
3074       IF AudioGear=3 AND Kmh<=72 THEN NextGear=2
3076       IF AudioGear=4 AND Kmh<=112 THEN NextGear=3
3078       IF AudioGear=5 AND Kmh<=156 THEN NextGear=4
3080     ENDIF
3082     IF NextGear<>AudioGear THEN AudioGear=NextGear:AudioShift=0.25
3084     GearTop=45
3086     IF AudioGear=2 THEN GearTop=85
3088     IF AudioGear=3 THEN GearTop=130
3090     IF AudioGear=4 THEN GearTop=180
3092     IF AudioGear=5 THEN GearTop=255
3094     Rpm=0.72+2.25*Kmh/GearTop+Throttle*0.12
3096   ENDIF
3098   Rpm=MIN(3.2,Rpm)
3100   IF AudioShift>0.15 THEN EngineVol=EngineVol*0.75
3102   AUDIO RATE 1,Rpm,20
3104   AUDIO VOLUME 1,EngineVol,20
3106   ' Match the tyre-mark threshold; normal rolling makes no mud sound.
3108   DriftVol=0
3110   IF Air=0 AND ABS(SlipR)>0.12 AND Speed>80 THEN DriftVol=MIN(0.23,1.15*((ABS(SlipR)-0.12)*0.28+Speed/6500))
3112   AUDIO VOLUME 2,DriftVol,10
3120 SUBEND
3130 ' ----------------------------------------------------------------------
3140 DEF SUB togglePause
3150   LOCAL PauseGap
3160   IF Paused=0 THEN
3170     Paused=1 : PauseStart=TIME
3190     EXIT SUB
3200   ENDIF
3210   Paused=0
3220   PauseGap=TIME-PauseStart
3230   PauseTotal=PauseTotal+PauseGap
3240   ReadyUntil=ReadyUntil+PauseGap
3250   PrevT=TIME : Elapsed=0
3270 SUBEND
3280 ' ----------------------------------------------------------------------
3290 DEF SUB readControls
3300   Throttle=MAX(ABS(KEYDOWN(30)),ABS(KEYDOWN(87)))
3310   Brake=MAX(ABS(KEYDOWN(31)),ABS(KEYDOWN(83)))
3320   Turn=MAX(ABS(KEYDOWN(29)),ABS(KEYDOWN(68)))-MAX(ABS(KEYDOWN(28)),ABS(KEYDOWN(65)))
3330   Handbrake=ABS(KEYDOWN(32))
3340 SUBEND
3350 ' ----------------------------------------------------------------------
3360 DEF SUB driveDemo
3370   LOCAL DemoIndex, Look, Ahead, Dj, Dx, Dy
3380   LOCAL Desired, VelAngle, ErrorAng, PursuitYaw, DemoRate, Curve
3390   LOCAL Dk, J, J2, TargetSpeed
3400   ' Demo steers toward the next part of the track using the same normal inputs.
3410   Qx=Px : Qy=Py
3430   CALL findTrack
3440   DemoIndex=NearI
3450   Look=MAX(80,MIN(330,Speed*0.43))
3460   Ahead=INT(Look/25)+1
3470   Dj=(DemoIndex+Ahead) MOD Np
3480   Dx=TrX(Dj)-Px : Dy=TrY(Dj)-Py
3500   Desired=FNA2(Dy,Dx)
3510   VelAngle=Ang
3520   IF Speed>25 THEN VelAngle=FNA2(Vy,Vx)
3550   ErrorAng=FNWRAP(Desired-VelAngle)
3560   PursuitYaw=2*Speed*SIN(ErrorAng)/Look
3570   DemoRate=0.9*(2+Speed/220)/(1+(Speed/1200)^2)*MIN(1,MAX(0.05,Speed/95))
3580   Turn=MAX(-1,MIN(1,(PursuitYaw+(PursuitYaw-Yaw)*0.35)/DemoRate))
3590   Curve=0
3600   FOR Dk=0 TO 14
3610     J=(DemoIndex+Dk) MOD Np
3620     J2=(J+2) MOD Np
3630     Curve=MAX(Curve,ABS(TrDx(J)*TrDy(J2)-TrDy(J)*TrDx(J2))/50)
3640   NEXT Dk
3650   TargetSpeed=MIN(780,SQR(1200/MAX(0.001,Curve)))
3660   IF RAlong>-220 AND RAlong<Rl AND ABS(RSide)<100 THEN TargetSpeed=MIN(TargetSpeed,620)
3690   IF ABS(ErrorAng)>0.85 THEN TargetSpeed=MIN(TargetSpeed,240)
3720   Throttle=0 : Brake=0 : Handbrake=0
3750   IF Speed<TargetSpeed THEN Throttle=1
3780   IF Speed>TargetSpeed+18 THEN Brake=1
3810 SUBEND
3820 ' ----------------------------------------------------------------------
3830 DEF SUB handleCommands
3840   ' Discrete commands; D remains steering during a manually driven race.
3850   IF K$="M" THEN
3860     Muted=1-Muted
3870     CALL resetAudio
3880   ENDIF
3890   IF K$="G" THEN GhostShow=1-GhostShow
3920   IF K$="P" AND Phase>0 THEN CALL togglePause
3950   IF K$="R" THEN
3960     Demo=0
3970     CALL resetAttempt
3980     CALL loadPersonalBest
3990     Phase=1 : ReadyUntil=TIME+3
4010     PrevT=TIME : Accum=0
4030   ENDIF
4040   IF Phase=0 AND (K$=CHR$(13) OR K$=" ") THEN
4050     Demo=0 : Phase=1 : ReadyUntil=TIME+3
4080     CALL resetAudio
4090   ENDIF
4100   IF K$="D" AND (Phase=0 OR Demo) THEN
4110     Demo=1-Demo
4120     CALL resetAttempt
4130     Phase=2
4140     IF Demo=0 THEN
4150       CALL loadPersonalBest
4160       Msg$="CROSS THE START LINE TO BEGIN" : MsgT=TIME+4
4180     ENDIF
4190   ENDIF
4200   IF Phase=1 AND TIME>=ReadyUntil AND Paused=0 THEN
4210     Phase=2
4220     Msg$="CROSS THE START LINE TO BEGIN" : MsgT=TIME+4
4240   ENDIF
4250 SUBEND
4260 ' ----------------------------------------------------------------------
4270 DEF SUB stepCar
4280   LOCAL Ca, Sa, SteerRate, YawRate, WheelAngle, SlipF
4290   LOCAL Grip, Travel, Drive, NewSpeed, Motion, TyreAxis
4300   LOCAL SideSlip, AlignRate, TargetYaw, OldAlong, TrackForward
4310   ' Drift model: steering turns the body; tyre force rotates velocity after it.
4320   OldPx=Px : OldPy=Py : OldZ=Z : OldAir=Air : OldAng=Ang
4370   Qx=Px : Qy=Py
4390   CALL sampleTerrain
4400   Ca=COS(Ang) : Sa=SIN(Ang)
4420   Vf=Vx*Ca+Vy*Sa : Vl=-Vx*Sa+Vy*Ca
4440   ' Short taps settle promptly; held steering still sustains a natural drift.
4450   Speed=SQR(Vx*Vx+Vy*Vy)
4460   SteerRate=32 : YawRate=10
4480   IF Turn=0 THEN SteerRate=48:YawRate=18
4520   IF Turn*Yaw<0 THEN YawRate=12
4550   Steer=Steer+(Turn-Steer)*MIN(1,SteerRate*H)
4560   WheelAngle=Steer*0.65/(1+ABS(Vf)/1000)
4570   SlipF=ATN((Vl+Axle*Yaw)/(ABS(Vf)+26))-WheelAngle*SGN(Vf)
4580   SlipR=ATN((Vl-Axle*Yaw)/(ABS(Vf)+26))
4590   Slip=MIN(1,ABS(SlipR)*2)
4600   Grip=1
4610   IF NearD>83 THEN Grip=0.78
4640   Travel=SGN(Vf)
4650   IF Speed<0.01 THEN Travel=1
4680   Drive=Throttle*260/(1+MAX(0,Vf)/1200)
4690   IF Brake THEN
4700     Drive=-900*SGN(Vf)
4710     IF Vf<8 THEN
4720       Drive=-180
4730       IF Vf<-85 THEN Drive=0
4760     ENDIF
4770   ENDIF
4780   Drive=Drive-Handbrake*110*SGN(Vf)-Speed*Travel*(0.07+Speed*0.000267)
4790   IF Throttle=0 AND Brake=0 THEN Drive=Drive-Travel*MIN(9,Speed/H)
4820   NewSpeed=Speed*Travel+Drive*H
4830   Motion=Ang
4840   IF Speed>0.01 THEN Motion=FNA2(Vy,Vx)
4870   TyreAxis=Ang
4880   IF NewSpeed<0 THEN TyreAxis=Ang+PI
4910   IF Speed<0.01 OR NewSpeed*Travel<0 THEN Motion=TyreAxis
4940   SideSlip=FNWRAP(Motion-TyreAxis)
4950   AlignRate=(7-Throttle*2)*(1-Handbrake*0.72)*Grip
4960   TargetYaw=Steer*0.9*(2+Speed/220)/(1+(Speed/1200)^2)*MIN(1,Speed/95)*Travel
4970   TargetYaw=TargetYaw*(1+Handbrake*0.35)/(1+MAX(0,ABS(SideSlip)-0.85)*2)
4980   IF Air THEN
4990     Drive=0
5000     NewSpeed=Speed*Travel
5010     Yaw=Yaw/(1+0.25*H)
5020   ELSE
5030     Motion=Motion-SideSlip*MIN(1,AlignRate*H)
5040     NewSpeed=NewSpeed/(1+ABS(SideSlip)*0.035*H)
5050     Vx=ABS(NewSpeed)*COS(Motion) : Vy=ABS(NewSpeed)*SIN(Motion)
5070     Yaw=Yaw+(TargetYaw-Yaw)*MIN(1,YawRate*H)
5080     Vx=Vx-392.4*SlopeX*H : Vy=Vy-392.4*SlopeY*H
5100   ENDIF
5110   IF Speed<0.01 AND Throttle=0 AND Brake=0 THEN
5120     Vx=0 : Vy=0 : Yaw=0
5150   ENDIF
5160   Yaw=MAX(-5.5,MIN(5.5,Yaw))
5170   Ang=Ang+Yaw*H
5180   Ang=Ang-2*PI*INT(Ang/(2*PI))
5190   Px=MAX(20,MIN(MapW-20,Px+Vx*H)) : Py=MAX(20,MIN(WorldH-20,Py+Vy*H))
5210   Qx=Px : Qy=Py
5230   CALL sampleTerrain
5240   ' The vertical lip is solid when approached backwards below the ramp deck.
5250   OldAlong=(OldPx-Rx)*RTx+(OldPy-Ry)*RTy
5260   IF OldAlong>=Rl AND RAlong<Rl AND RAlong>Rl-10 AND ABS(RSide)<Rw AND Z<GroundZ+Rh-5 THEN CALL hitRampBack
5290   Z=Z+Vz*H : Vz=Vz-392.4*H
5310   Air=ABS(Z-FloorZ>0.65)
5320   IF Z<=FloorZ THEN CALL landCar
5350   MaxHeight=MAX(MaxHeight,Z)
5360   IF OldAir=0 AND Air AND Vz>10 THEN Jumps=Jumps+1
5390   IF Z-FloorZ<8 THEN CALL collideBarriers
5420   Qx=Px : Qy=Py
5440   CALL findTrack
5450   IF NearD>145 AND Air=0 THEN Invalid=1
5480   TrackForward=Vx*TrDx(NearI)+Vy*TrDy(NearI)
5490   IF TrackForward<-30 THEN WrongTime=WrongTime+H ELSE WrongTime=MAX(0,WrongTime-H*2)
5540   HitCool=MAX(0,HitCool-H) : Rumble=MAX(0,Rumble-H*5)
5560   Speed=SQR(Vx*Vx+Vy*Vy)
5570 SUBEND
5580 ' ----------------------------------------------------------------------
5590 DEF SUB findTrack
5600   LOCAL Gc, Gr, Seed, BestD2, Ni, Nn
5610   LOCAL Sx, Sy, Along, Nx, Ny, Dx
5620   LOCAL Dy, Dd
5630   ' Coarse spatial lookup + exact projection onto five nearby line segments.
5640   Gc=MAX(0,MIN(GridW-1,INT(Qx/GridStep))) : Gr=MAX(0,MIN(GridH-1,INT(Qy/GridStep)))
5660   Seed=VAL(MID$(Grid$(Gr),Gc*3+1,3))
5670   BestD2=1E+20
5680   FOR Ni=Seed-2 TO Seed+2
5690     Nn=(Ni+Np) MOD Np
5700     Sx=Qx-TrX(Nn) : Sy=Qy-TrY(Nn)
5720     Along=MAX(0,MIN(TrL(Nn),Sx*TrDx(Nn)+Sy*TrDy(Nn)))
5730     Nx=TrX(Nn)+TrDx(Nn)*Along : Ny=TrY(Nn)+TrDy(Nn)*Along
5750     Dx=Qx-Nx : Dy=Qy-Ny
5770     Dd=Dx*Dx+Dy*Dy
5780     IF Dd<BestD2 THEN
5790       BestD2=Dd : NearI=Nn : NearX=Nx : NearY=Ny : NearAlong=Along
5840     ENDIF
5850   NEXT Ni
5860   NearD=SQR(BestD2)
5870 SUBEND
5880 ' ----------------------------------------------------------------------
5890 DEF SUB sampleTerrain
5900   LOCAL Si, Sj, TerrainFrac, TrackSide, Bank, CentreZ
5910   LOCAL TerrainSide, AlongSlope, TerrainFade, ShoulderDrop
5920   ' Art and tyres share interpolated centre height and signed crossfall.
5930   CALL findTrack
5940   Si=NearI : Sj=(Si+1) MOD Np
5960   TerrainFrac=NearAlong/TrL(Si)
5970   TrackSide=-(Qx-NearX)*TrDy(Si)+(Qy-NearY)*TrDx(Si)
5980   Bank=TrBank(Si)+(TrBank(Sj)-TrBank(Si))*TerrainFrac
5990   CentreZ=TrZ(Si)+(TrZ(Sj)-TrZ(Si))*TerrainFrac
6000   TerrainSide=MAX(-109,MIN(109,TrackSide))
6010   GroundZ=CentreZ+Bank*TerrainSide
6020   AlongSlope=(TrZ(Sj)-TrZ(Si)+(TrBank(Sj)-TrBank(Si))*TerrainSide)/TrL(Si)
6030   SlopeX=AlongSlope*TrDx(Si)-Bank*TrDy(Si) : SlopeY=AlongSlope*TrDy(Si)+Bank*TrDx(Si)
6050   IF NearD>109 THEN
6060     SlopeX=AlongSlope*TrDx(Si) : SlopeY=AlongSlope*TrDy(Si)
6080     TerrainFade=MAX(0,(185-NearD)/76)
6090     ShoulderDrop=GroundZ/76*SGN(TrackSide)
6100     SlopeX=SlopeX*TerrainFade+ShoulderDrop*TrDy(Si) : SlopeY=SlopeY*TerrainFade-ShoulderDrop*TrDx(Si)
6120     GroundZ=GroundZ*TerrainFade
6130     IF NearD>=185 THEN SlopeX=0:SlopeY=0
6170   ENDIF
6180   FloorZ=GroundZ
6190   RAlong=(Qx-Rx)*RTx+(Qy-Ry)*RTy : RSide=-(Qx-Rx)*RTy+(Qy-Ry)*RTx
6210   IF RAlong>=0 AND RAlong<Rl AND ABS(RSide)<Rw THEN
6220     FloorZ=FloorZ+Rh*RAlong/Rl
6230     SlopeX=SlopeX+RTx*Rh/Rl : SlopeY=SlopeY+RTy*Rh/Rl
6250   ENDIF
6260 SUBEND
6270 ' ----------------------------------------------------------------------
6280 DEF SUB landCar
6290   IF OldAir AND Vz<-18 THEN
6300     LandImpact=-Vz
6310     Vx=Vx*0.975 : Vy=Vy*0.975
6330     Rumble=MIN(1,LandImpact/100)
6340     CALL playImpact
6350   ENDIF
6360   Z=FloorZ : Vz=0 : Air=0
6390   Vz=Vx*SlopeX+Vy*SlopeY
6400 SUBEND
6410 ' ----------------------------------------------------------------------
6420 DEF SUB hitRampBack
6430   LOCAL Impulse
6440   Px=OldPx : Py=OldPy
6460   Impulse=(Vx*RTx+Vy*RTy)*1.22
6470   Vx=Vx-Impulse*RTx : Vy=Vy-Impulse*RTy
6490   FloorZ=GroundZ
6500   CALL playImpact
6510 SUBEND
6520 ' ----------------------------------------------------------------------
6530 DEF SUB collideBarriers
6540   LOCAL Ca, Sa, Contact, LeverX, LeverY, NormalX
6550   LOCAL NormalY, Depth, Vn, Torque, Impulse
6560   ' Three discs form a capsule. Contact impulses change translation AND rotation.
6570   Ca=COS(Ang) : Sa=SIN(Ang)
6590   FOR Contact=-1 TO 1
6600     LeverX=Contact*ContactArm*Ca : LeverY=Contact*ContactArm*Sa
6620     Qx=Px+LeverX : Qy=Py+LeverY
6640     CALL findTrack
6650     IF NearD>WallContact THEN
6660       NormalX=(Qx-NearX)/NearD : NormalY=(Qy-NearY)/NearD
6680       Depth=MIN(NearD-WallContact,40)
6690       Px=Px-NormalX*Depth : Py=Py-NormalY*Depth
6710       Vn=(Vx-Yaw*LeverY)*NormalX+(Vy+Yaw*LeverX)*NormalY
6720       IF Vn>0 THEN
6730         Torque=LeverX*NormalY-LeverY*NormalX
6740         Impulse=-1.28*Vn/(1+Torque*Torque/Inertia)
6750         Vx=Vx+Impulse*NormalX : Vy=Vy+Impulse*NormalY : Yaw=Yaw+Impulse*Torque/Inertia
6780         Vx=Vx*0.96 : Vy=Vy*0.96
6800         Rumble=MIN(1,Vn/120)
6810         IF Vn>20 AND HitCool=0 THEN
6820           Collisions=Collisions+1
6830           CALL playImpact
6840           HitCool=0.12
6850         ENDIF
6860       ENDIF
6870     ENDIF
6880   NEXT Contact
6890 SUBEND
6900 ' ----------------------------------------------------------------------
6910 DEF SUB playImpact
6920   IF Muted OR AudioReady=0 THEN EXIT SUB
6950   AUDIO PLAY 3,3,0,MIN(0.65,0.15+Rumble*0.5)
6960 SUBEND
6970 ' ----------------------------------------------------------------------
6980 DEF SUB updateTiming
6990   ' Ordered controls include the legal verge and airborne drift after the ramp.
7000   Gi=NextGate
7010   CALL testGate
7020   IF GateCross THEN
7030     IF Gi=0 THEN CALL completeLap ELSE CALL recordSplit
7080     NextGate=(Gi+1) MOD NGates
7090   ELSE
7100     ' A missed control cannot merge two attempts into a single timed lap.
7110     IF Started=0 OR NextGate=0 THEN EXIT SUB
7140     Gi=0
7150     CALL testGate
7160     IF GateCross=0 THEN EXIT SUB
7190     Msg$="INCOMPLETE LAP - TRY AGAIN" : MsgT=TIME+4
7210     LapStart=CrossTime : LapTime=0 : Invalid=0
7240     MAT Splits=0
7250     SplitDelta=0 : NextGate=1
7270     CALL startGhost
7280   ENDIF
7290 SUBEND
7300 ' ----------------------------------------------------------------------
7310 DEF SUB completeLap
7320   IF Started=0 THEN
7330     Started=1 : LapStart=CrossTime : Invalid=0
7360     CALL startGhost
7370     EXIT SUB
7380   ENDIF
7390   LastLap=CrossTime-LapStart : Laps=Laps+1
7410   Splits(3)=LastLap
7420   LastDelta=0
7430   IF Best>0 THEN LastDelta=LastLap-Best
7460   Msg$="LAP "+FNCLOCK$(LastLap) : MsgT=TIME+4
7480   SessionImproved=0
7490   IF Invalid THEN
7500     Msg$="INVALID LAP - TRY AGAIN"
7510   ELSE
7520     IF Demo=0 THEN CALL updateSectorBests(Splits,SectorBest)
7550     IF Demo=0 AND (SessionBest=0 OR LastLap<SessionBest) THEN CALL publishGhost
7580     IF Best=0 OR LastLap<Best THEN CALL updatePersonalBest
7610     IF AudioReady AND Muted=0 THEN AUDIO PLAY 4,4,0,0.35+SessionImproved*0.2,0,1+SessionImproved*0.35
7640   ENDIF
7650   LapStart=CrossTime : Invalid=0
7670   MAT Splits=0
7680   SplitDelta=0
7690   CALL startGhost
7700 SUBEND
7710 ' ----------------------------------------------------------------------
7720 DEF SUB updatePersonalBest
7730   Best=LastLap
7740   MAT Bests=Splits
7750   Msg$="NEW PERSONAL BEST  "+FNCLOCK$(Best)
7760   IF Demo=0 THEN CALL savePersonalBest
7790 SUBEND
7800 ' ----------------------------------------------------------------------
7810 DEF SUB recordSplit
7820   IF Gi=4 THEN
7830     Splits(1)=CrossTime-LapStart
7840     IF Best>0 THEN SplitDelta=Splits(1)-Bests(1)
7870   ENDIF
7880   IF Gi=4 THEN
7890     SectorDone=1
7900     CALL compareSector
7910   ENDIF
7920   IF Gi=8 THEN
7930     Splits(2)=CrossTime-LapStart
7940     IF Best>0 THEN SplitDelta=Splits(2)-Bests(2)
7970   ENDIF
7980   IF Gi=8 THEN
7990     SectorDone=2
8000     CALL compareSector
8010   ENDIF
8020 SUBEND
8030 ' ----------------------------------------------------------------------
8040 DEF SUB testGate
8050   LOCAL Ogd, Ngd, Lateral
8060   ' Test the swept crossing, not its endpoint; use the same 145 px legal margin.
8070   GateCross=0
8080   Ogd=(OldPx-GateX(Gi))*GateDx(Gi)+(OldPy-GateY(Gi))*GateDy(Gi)
8090   Ngd=(Px-GateX(Gi))*GateDx(Gi)+(Py-GateY(Gi))*GateDy(Gi)
8100   IF Ogd>=0 OR Ngd<0 OR Ngd-Ogd<0.000001 THEN EXIT SUB
8130   CrossFrac=-Ogd/(Ngd-Ogd)
8140   CrossX=OldPx+(Px-OldPx)*CrossFrac : CrossY=OldPy+(Py-OldPy)*CrossFrac
8160   Lateral=ABS(-(CrossX-GateX(Gi))*GateDy(Gi)+(CrossY-GateY(Gi))*GateDx(Gi))
8170   IF Lateral>145 THEN EXIT SUB
8200   GateCross=1
8210   CrossTime=RaceNow-H*(1-CrossFrac)
8220 SUBEND
8230 ' ----------------------------------------------------------------------
8240 DEF SUB updateMud
8250   CALL updateTiming
8260   MarkT=MarkT+H : DustT=DustT+H : MudClock=MudClock+H
8290   CALL emitMud
8300   CALL emitDust
8310 SUBEND
8320 ' ----------------------------------------------------------------------
8330 DEF SUB emitDust
8340   IF DustT<0.045 OR Air OR Speed<65 THEN EXIT SUB
8370   DustT=0
8380   DustIndex=(DustIndex+1) MOD MaxDust
8390   DustParticle=DustIndex
8400   DustX(DustParticle)=Px-COS(Ang)*18*CarScale : DustY(DustParticle)=Py-SIN(Ang)*18*CarScale
8420   DustVx(DustParticle)=Vx*0.12+(RND-0.5)*14 : DustVy(DustParticle)=Vy*0.12+(RND-0.5)*14
8440   DustL(DustParticle)=0.5+Slip*0.55
8450   DustAge(DustParticle)=0 : DustZ(DustParticle)=FloorZ
8470 SUBEND
8480 ' ----------------------------------------------------------------------
8490 DEF SUB projectMud
8500   LOCAL KeepZ, KeepGround, KeepSx, KeepSy, KeepRAl, KeepRS
8510   LOCAL MudZ
8520   ' Project each deposit onto the local bank/deck, preserving the car contact.
8530   KeepZ=FloorZ : KeepGround=GroundZ
8550   KeepSx=SlopeX : KeepSy=SlopeY
8570   KeepRAl=RAlong : KeepRS=RSide
8590   Qx=MWx : Qy=MWy
8610   CALL sampleTerrain
8620   MudZ=FloorZ
8630   IF NearD>=100 AND NearD<=109 THEN MudZ=MudZ+7
8660   MudY=MWy*Iso-MudZ*ZProj
8670   FloorZ=KeepZ : GroundZ=KeepGround
8690   SlopeX=KeepSx : SlopeY=KeepSy
8710   RAlong=KeepRAl : RSide=KeepRS
8730 SUBEND
8740 ' ----------------------------------------------------------------------
8750 DEF SUB unlinkPatch
8760   IF MPrev(Mi)>0 THEN MNext(MPrev(Mi))=MNext(Mi) ELSE Head(OldMC)=MNext(Mi)
8810   IF MNext(Mi)>0 THEN MPrev(MNext(Mi))=MPrev(Mi)
8840   MGrid(MGx(Mi),MGy(Mi))=0
8850 SUBEND
8860 ' ----------------------------------------------------------------------
8870 DEF SUB stampRut
8880   LOCAL RutDx, RutDy, RutLen, RutSteps, Rs
8890   ' Sample wheel motion finely; the direct index gives one owner per soil patch.
8900   RutDx=RutX1-RutX0 : RutDy=RutY1-RutY0
8920   RutLen=SQR(RutDx^2+RutDy^2)
8930   IF RutLen<0.15 OR RutLen>75 THEN EXIT SUB
8960   RutUx=RutDx/RutLen : RutUy=RutDy/RutLen
8980   RutSteps=INT(RutLen/2.5)+1
8990   FOR Rs=0 TO RutSteps
9000     RGx=INT((RutX0+RutDx*Rs/RutSteps)/MudStep) : RGy=INT((RutY0+RutDy*Rs/RutSteps)/MudStep)
9020     IF RGx>=0 AND RGx<MudW AND RGy>=0 AND RGy<MudH THEN
9030       Mi=MGrid(RGx,RGy)
9040       IF Mi=0 THEN CALL allocatePatch
9070       IF MStamp(Mi)<>RutStamp THEN
9080         MStamp(Mi)=RutStamp
9090         CALL remixPatch
9100       ENDIF
9110     ENDIF
9120   NEXT Rs
9130 SUBEND
9140 ' ----------------------------------------------------------------------
9150 DEF SUB allocatePatch
9160   LOCAL OldMC, CellX, CellY, Cell
9170   MarkIndex=MarkIndex MOD MaxMark+1
9180   Mi=MarkIndex
9190   OldMC=MCell(Mi)
9200   IF OldMC>0 THEN CALL unlinkPatch
9230   MGx(Mi)=RGx : MGy(Mi)=RGy
9250   MGrid(RGx,RGy)=Mi
9260   M1x(Mi)=(RGx+0.5)*MudStep : M1y(Mi)=(RGy+0.5)*MudStep
9280   CellX=INT(M1x(Mi)/256) : CellY=INT(M1y(Mi)/256)
9300   Cell=CellY*TileCols+CellX+1
9310   MCell(Mi)=Cell
9320   MPrev(Mi)=0
9330   MNext(Mi)=Head(Cell)
9340   IF Head(Cell)>0 THEN MPrev(Head(Cell))=Mi
9370   Head(Cell)=Mi
9380   MDepth(Mi)=0 : MChurn(Mi)=0
9400   MStamp(Mi)=-1 : MTouch(Mi)=MudClock : MVar(Mi)=0 : MFade(Mi)=1
9440   MDirX(Mi)=RutUx : MDirY(Mi)=RutUy
9460   MXMin(Mi)=M1x(Mi)-7 : MXMax(Mi)=M1x(Mi)+7
9480   MYMin(Mi)=M1y(Mi)-7 : MYMax(Mi)=M1y(Mi)+7
9500   MarkCount=MIN(MaxMark,MarkCount+1)
9510 SUBEND
9520 ' ----------------------------------------------------------------------
9530 DEF SUB remixPatch
9540   LOCAL MudAge, DotRut, CrossRut, DirSign, MixWeight, DirX
9550   LOCAL DirY, DirLen, MudNoise, Dep, Churn, MudTone
9560   ' Aligned passes deepen; crossing passes refill the old cut and churn its soil.
9570   MudAge=MAX(0,MudClock-MTouch(Mi))
9580   ' Reshape only on a new visit, never at every tyre sample or render frame.
9590   IF MudAge>0.4 THEN CALL renewPatch
9620   MTouch(Mi)=MudClock : MFade(Mi)=1
9640   DotRut=MDirX(Mi)*RutUx+MDirY(Mi)*RutUy
9650   CrossRut=1-ABS(DotRut)
9660   MDepth(Mi)=MDepth(Mi)*(1-0.55*CrossRut)
9670   MDepth(Mi)=MIN(1,MDepth(Mi)+RutCut*(1-MDepth(Mi)))
9680   MChurn(Mi)=MIN(1,MChurn(Mi)*0.94+CrossRut*0.55)
9690   DirSign=1
9700   IF DotRut<0 THEN DirSign=-1
9730   MixWeight=0.35+0.55*CrossRut
9740   DirX=MDirX(Mi)*(1-MixWeight)+RutUx*DirSign*MixWeight : DirY=MDirY(Mi)*(1-MixWeight)+RutUy*DirSign*MixWeight
9760   DirLen=SQR(DirX^2+DirY^2)
9770   MDirX(Mi)=DirX/DirLen : MDirY(Mi)=DirY/DirLen
9790   ' First-pass clods keep their shape; later visits remix this small soil patch.
9800   MudNoise=(MGx(Mi)*73+MGy(Mi)*137+MVar(Mi)) MOD 97
9810   M1x(Mi)=MGx(Mi)*MudStep+2+MudNoise MOD 3 : M1y(Mi)=MGy(Mi)*MudStep+2+(MudNoise\3) MOD 3
9830   MXMin(Mi)=M1x(Mi)-8 : MXMax(Mi)=M1x(Mi)+8
9850   MYMin(Mi)=M1y(Mi)-8 : MYMax(Mi)=M1y(Mi)+8
9870   M2x(Mi)=3+MudNoise MOD 4 : M2y(Mi)=2+(MudNoise\4) MOD 4
9890   M3x(Mi)=(MudNoise\7) MOD 7-3 : M3y(Mi)=(MudNoise\11) MOD 7-3
9910   Dep=MDepth(Mi) : Churn=MChurn(Mi)
9930   MudTone=MudNoise MOD 17-8
9940   MCol(Mi)=RGB(139-31*Dep+17*Churn+MudTone,92-24*Dep+13*Churn+MudTone,58-18*Dep+9*Churn+MudTone)
9950   MEdge(Mi)=RGB(176+8*Dep-9*Churn+MudTone,126+6*Dep-7*Churn+MudTone,84+5*Dep-5*Churn+MudTone)
9960   MBase(Mi)=RGB(155-15*Dep+13*Churn+MudTone,107-11*Dep+10*Churn+MudTone,69-8*Dep+7*Churn+MudTone)
9970 SUBEND
9980 ' ----------------------------------------------------------------------
9990 DEF SUB renewPatch
10000   ' Separate visual randomness leaves dust and driving inputs undisturbed.
10010   MudSeed=(MudSeed*16807) MOD 2147483647
10020   MVar(Mi)=1+INT(MudSeed*96/2147483647)
10030   MDepth(Mi)=MDepth(Mi)*MAX(0,1-MudAge/120)
10040   MChurn(Mi)=MChurn(Mi)*MAX(0,1-MudAge/80)
10050 SUBEND
10060 ' ----------------------------------------------------------------------
10070 DEF SUB updateDust
10080   LOCAL DustParticle
10090   FOR DustParticle=0 TO MaxDust-1
10100     IF DustL(DustParticle)>0 THEN
10110       DustL(DustParticle)=MAX(0,DustL(DustParticle)-H) : DustAge(DustParticle)=DustAge(DustParticle)+H
10130       DustX(DustParticle)=DustX(DustParticle)+DustVx(DustParticle)*H : DustY(DustParticle)=DustY(DustParticle)+DustVy(DustParticle)*H
10150     ENDIF
10160   NEXT DustParticle
10170 SUBEND
10180 ' ----------------------------------------------------------------------
10190 DEF SUB readCarPose
10200   LOCAL GSlope
10210   ' Read the player's sprite pose without modifying any driving state.
10220   GPose=0
10230   IF Air AND Vz>10 THEN GPose=1
10260   IF Air AND Vz<-10 THEN GPose=2
10290   GSlope=SlopeX*COS(Ang)+SlopeY*SIN(Ang)
10300   IF Air=0 AND GSlope>0.035 THEN GPose=1
10330   IF Air=0 AND GSlope<-0.035 THEN GPose=2
10360 SUBEND
10370 ' ----------------------------------------------------------------------
10380 DEF SUB startGhost
10390   ' Start at the exact finish crossing; restart preserves the best ghost.
10400   RecN=0 : RecFull=0
10420   GhostPlay=0 : SectorDone=0 : MiniView=0
10450   RecNext=1/30
10460   IF Demo THEN EXIT SUB
10490   CALL readCarPose
10500   Rt=0
10510   RxPos=CrossX : RyPos=CrossY
10530   RzPos=OldZ+(Z-OldZ)*CrossFrac
10540   RAngle=OldAng+FNWRAP(Ang-OldAng)*CrossFrac
10550   CALL appendGhost
10560 SUBEND
10570 ' ----------------------------------------------------------------------
10580 DEF SUB recordGhost
10590   ' Timestamped samples at up to 30 Hz, even while the ghost is hidden.
10600   IF Demo OR Started=0 OR Invalid OR RecFull THEN EXIT SUB
10630   Rt=MAX(0,RaceNow-LapStart)
10640   IF Rt<RecNext THEN EXIT SUB
10670   RecNext=Rt+1/30
10680   RxPos=Px : RyPos=Py : RzPos=Z
10710   RAngle=Ang
10720   CALL readCarPose
10730   CALL appendGhost
10740 SUBEND
10750 ' ----------------------------------------------------------------------
10760 DEF SUB appendGhost
10770   IF RecN>=MaxGhost THEN
10780     RecFull=1
10790     EXIT SUB
10800   ENDIF
10810   IF RecN>0 THEN
10820     IF Rt<=Gt(RecBank,RecN-1) THEN EXIT SUB
10850   ENDIF
10860   Gt(RecBank,RecN)=Rt
10870   Gx(RecBank,RecN)=RxPos : Gy(RecBank,RecN)=RyPos : Gz(RecBank,RecN)=RzPos
10900   Ga(RecBank,RecN)=RAngle
10910   Gf(RecBank,RecN)=FloorZ : Gp(RecBank,RecN)=GPose
10930   RecN=RecN+1
10940 SUBEND
10950 ' ----------------------------------------------------------------------
10960 DEF SUB publishGhost
10970   SessionBest=LastLap
10980   SessionImproved=1
10990   Msg$="SESSION BEST  "+FNCLOCK$(LastLap)
11000   Rt=LastLap
11010   RxPos=CrossX : RyPos=CrossY
11030   RzPos=OldZ+(Z-OldZ)*CrossFrac
11040   RAngle=OldAng+FNWRAP(Ang-OldAng)*CrossFrac
11050   CALL readCarPose
11060   CALL appendGhost
11070   GhostN=0
11080   IF RecFull OR RecN<2 THEN EXIT SUB
11110   GhostN=RecN : GhostBank=RecBank
11130   RecBank=1-GhostBank : GhostPlay=0
11150 SUBEND
11160 ' ----------------------------------------------------------------------
11170 DEF SUB sampleGhost
11180   LOCAL Gj, GFrac
11190   ' Replay is purely visual. Interpolate position and take the shortest angle.
11200   GhostVisible=0
11210   IF GhostShow=0 OR GhostN<2 OR Demo OR Started=0 THEN EXIT SUB
11240   IF LapTime<0 OR LapTime>Gt(GhostBank,GhostN-1) THEN EXIT SUB
11270   WHILE GhostPlay<GhostN-2 AND Gt(GhostBank,GhostPlay+1)<LapTime
11280     GhostPlay=GhostPlay+1
11290   WEND
11300   Gj=GhostPlay+1
11310   GFrac=MAX(0,MIN(1,(LapTime-Gt(GhostBank,GhostPlay))/(Gt(GhostBank,Gj)-Gt(GhostBank,GhostPlay))))
11320   GhostX=Gx(GhostBank,GhostPlay)+(Gx(GhostBank,Gj)-Gx(GhostBank,GhostPlay))*GFrac
11330   GhostY=Gy(GhostBank,GhostPlay)+(Gy(GhostBank,Gj)-Gy(GhostBank,GhostPlay))*GFrac
11340   GhostZ=Gz(GhostBank,GhostPlay)+(Gz(GhostBank,Gj)-Gz(GhostBank,GhostPlay))*GFrac
11350   GhostAngle=Ga(GhostBank,GhostPlay)+FNWRAP(Ga(GhostBank,Gj)-Ga(GhostBank,GhostPlay))*GFrac
11360   GhostFloor=Gf(GhostBank,GhostPlay)+(Gf(GhostBank,Gj)-Gf(GhostBank,GhostPlay))*GFrac
11370   GhostPose=Gp(GhostBank,GhostPlay)
11380   IF GFrac>=0.5 THEN GhostPose=Gp(GhostBank,Gj)
11410   GhostVisible=1
11420 SUBEND
11430 ' ----------------------------------------------------------------------
11440 DEF SUB compareSector
11450   ' Compare individual sectors with session bests from completed valid laps.
11460   SectorTime=Splits(SectorDone)-Splits(SectorDone-1)
11470   SectorDelta=0 : SectorColor=RGB(171,180,172) : MiniView=0
11500   SectorRef=SectorBest(SectorDone)
11510   IF Demo OR Invalid OR SectorRef<=0 THEN EXIT SUB
11540   SectorDelta=SectorTime-SectorRef
11550   MiniView=SectorDone*2 : SectorColor=SectorRed
11570   IF SectorDelta<0 THEN MiniView=MiniView-1:SectorColor=SectorGreen
11610 SUBEND
11620 ' ----------------------------------------------------------------------
11630 DEF SUB updateSectorBests(Times, BestSectors)
11640   LOCAL Sector, Duration
11650   FOR Sector=1 TO 3
11660     Duration=FNSECTORDURATION(Times,Sector)
11670     IF Duration>0 AND (BestSectors(Sector)=0 OR Duration<BestSectors(Sector)) THEN BestSectors(Sector)=Duration
11700   NEXT Sector
11710 SUBEND
11720 ' ----------------------------------------------------------------------
11730 DEF SUB buildMinimap(MapX, MapY, BaseColors, Views$)
11740   LOCAL Mm, Mv, MBx, MBy, Ms, Mc
11750   LOCAL Me, Mn
11760   ' Cache seven 160x112 map variants once, not every frame.
11770   BaseColors(1)=RGB(151,177,198)
11780   BaseColors(2)=RGB(207,184,141)
11790   BaseColors(3)=RGB(181,157,197)
11800   FOR Mm=0 TO Np-1
11810     MapX(Mm)=8+TrX(Mm)*144/MapW : MapY(Mm)=104-TrY(Mm)*96/WorldH
11830   NEXT Mm
11840   MapX(Np)=MapX(0) : MapY(Np)=MapY(0)
11860   SMALLFONT TRANSPARENT
11870   FOR Mv=0 TO 6
11880     MBx=(Mv MOD 4)*160 : MBy=600-(INT(Mv/4)+1)*112
11900     TRECTANGLE MiniMap$,MBx,MBy,MBx+160,MBy+112
11910     PENWIDTH 2
11920     FOR Ms=1 TO 3
11930       Mc=BaseColors(Ms)
11940       IF Mv=Ms*2-1 THEN Mc=SectorGreen
11970       IF Mv=Ms*2 THEN Mc=SectorRed
12000       Me=Np
12010       IF Ms<3 THEN Me=GateI(Ms*4)
12040       FOR Mm=GateI((Ms-1)*4) TO Me-1 STEP 3
12050         Mn=MIN(Me,Mm+3)
12060         DRAW MBx+MapX(Mm),MBy+MapY(Mm),MBx+MapX(Mn),MBy+MapY(Mn),Mc
12070       NEXT Mm
12080     NEXT Ms
12090     PENWIDTH 1
12100     FOR Ms=0 TO 2
12110       Mm=GateI(Ms*4)
12120       FCIRCLE MBx+MapX(Mm),MBy+MapY(Mm),3,RGB(18,29,29)
12130       FCIRCLE MBx+MapX(Mm),MBy+MapY(Mm),1,RGB(235,229,197)
12140       Mm=GateI(Ms*4+2)
12150       INK RGB(237,229,208) : MOVE MBx+MapX(Mm)+4,MBy+MapY(Mm)+11 : LABEL DEC$(Ms+1,"0")
12180     NEXT Ms
12190     Views$(Mv)=SPRITE$(MBx,MBy,MBx+159,MBy+111)
12200   NEXT Mv
12210 SUBEND
12220 ' ----------------------------------------------------------------------
12230 DEF SUB drawWorld
12240   LOCAL Ix, Iy, SoilPass, Ty, Tx, Cell
12250   LOCAL Mi, Mx, My, Dx, Dy, Ex
12260   LOCAL Ey, DustParticle, Dir, U, V, Sx
12270   LOCAL Sy, Shade, GroundY, Pose, ForwardSlope, Cv
12280   LOCAL Bounce
12290   ' One camera-sized UV window on the immutable, cached prerendered map.
12300   Ix=INT(Cx) : Iy=INT(Cy)
12320   TRECTANGLE Track$,0,0,800,600,Ix,MapH-Iy-600,Ix+800,MapH-Iy
12330   ORIGIN 0,0,0,799,535,34
12340   ' Opaque coarse pixels cover painted arrows, wooden planks and kerb colours.
12350   FOR SoilPass=0 TO 2
12360     FOR Ty=MAX(0,INT((Iy-30)/256)) TO MIN(TileRows-1,INT((Iy+630)/256))
12370       FOR Tx=MAX(0,INT((Ix-30)/256)) TO MIN(TileCols-1,INT((Ix+830)/256))
12380         Cell=Ty*TileCols+Tx+1
12390         Mi=Head(Cell)
12400         WHILE Mi>0
12410           IF MXMax(Mi)>=Ix AND MXMin(Mi)<=Ix+800 AND MYMax(Mi)>=Iy AND MYMin(Mi)<=Iy+600 THEN
12420             ' Untouched clods hold for 30 s, then fade over 60-90 s of driving.
12430             IF SoilPass=0 THEN MFade(Mi)=MAX(0,MIN(1,1-(MudClock-MTouch(Mi)-30)/(60+(MGx(Mi)*13+MGy(Mi)*7) MOD 31)))
12460             IF MFade(Mi)>0 THEN
12470               Mx=INT(M1x(Mi)-Ix) : My=INT(599-M1y(Mi)+Iy)
12490               Dx=M2x(Mi) : Dy=M2y(Mi)
12510               Ex=M3x(Mi) : Ey=M3y(Mi)
12530               IF SoilPass=0 THEN FRECTANGLE Mx-2,My-1,Mx+Dx-3,My+Dy-2,MCol(Mi) ALPHA MFade(Mi)
12560               IF SoilPass=1 AND (MGx(Mi)+MGy(Mi)) MOD 3=0 THEN
12570                 FRECTANGLE Mx+Ex-1,My+Ey-1,Mx+Ex+1,My+Ey+1,MBase(Mi) ALPHA MFade(Mi)
12580               ENDIF
12590               IF SoilPass=2 AND (MGx(Mi)*3+MGy(Mi)) MOD 5=0 THEN
12600                 FRECTANGLE Mx+Ex,My+Ey,Mx+Ex+2,My+Ey+1,MEdge(Mi) ALPHA MFade(Mi)
12610               ENDIF
12620             ENDIF
12630           ENDIF
12640           Mi=MNext(Mi)
12650         WEND
12660       NEXT Tx
12670     NEXT Ty
12680   NEXT SoilPass
12690   PENWIDTH 1
12700   FOR DustParticle=0 TO MaxDust-1
12710     IF DustL(DustParticle)>0 THEN
12720       Dx=DustX(DustParticle)-Ix : Dy=599-DustY(DustParticle)*Iso+Iy+DustZ(DustParticle)*ZProj+DustAge(DustParticle)*8
12740       IF Dx>=-20 AND Dx<=820 AND Dy>=20 AND Dy<=550 THEN
12750         FCIRCLE Dx,Dy,3+DustAge(DustParticle)*12,RGB(199,164,110),0.65 ALPHA MIN(0.22,DustL(DustParticle)*0.2)
12760       ENDIF
12770     ENDIF
12780   NEXT DustParticle
12790   CALL drawGhost
12800   ' The car pivots in true 3D sprite views; its shadow remains on the ground.
12810   Dir=INT(Ang*64/(2*PI)+0.5) MOD 64
12820   U=(Dir MOD 16)*64 : V=256-(Dir\16+1)*64
12840   Sx=Px-Ix : Sy=599-Py*Iso+Iy
12860   Shade=MAX(0.13,0.37-MAX(0,Z-FloorZ)*0.002)
12870   GroundY=Sy+FloorZ*ZProj
12880   TRECTANGLE Shadow$,Sx-32,GroundY-21,Sx+32,GroundY+43,U,V,U+64,V+64,RGB(255,0,255) ALPHA Shade
12890   Pose=0
12900   IF Air AND Vz>10 THEN Pose=1
12930   IF Air AND Vz<-10 THEN Pose=2
12960   ForwardSlope=SlopeX*COS(Ang)+SlopeY*SIN(Ang)
12970   IF Air=0 AND ForwardSlope>0.035 THEN Pose=1
13000   IF Air=0 AND ForwardSlope<-0.035 THEN Pose=2
13030   Cv=768-(Pose*4+Dir\16+1)*64
13040   Bounce=Rumble*SIN(TIME*45)*1.5
13050   TRECTANGLE Car$,Sx-32,Sy-21+Z*ZProj+Bounce,Sx+32,Sy+43+Z*ZProj+Bounce,U,Cv,U+64,Cv+64,RGB(255,0,255)
13060   ORIGIN
13070 SUBEND
13080 ' ----------------------------------------------------------------------
13090 DEF SUB drawGhost
13100   LOCAL GScreenX, GScreenY, GDir, Gu, Gv, GShadowY
13110   CALL sampleGhost
13120   IF GhostVisible=0 THEN EXIT SUB
13150   GScreenX=GhostX-Ix : GScreenY=599-GhostY*Iso+Iy
13170   IF GScreenX<-40 OR GScreenX>840 OR GScreenY+GhostZ*ZProj<-40 OR GScreenY+GhostZ*ZProj>640 THEN EXIT SUB
13200   GDir=INT(GhostAngle*64/(2*PI)+0.5) MOD 64
13210   IF GDir<0 THEN GDir=GDir+64
13240   Gu=(GDir MOD 16)*64 : Gv=256-(GDir\16+1)*64
13260   GShadowY=GScreenY+GhostFloor*ZProj
13270   TRECTANGLE Shadow$,GScreenX-32,GShadowY-21,GScreenX+32,GShadowY+43,Gu,Gv,Gu+64,Gv+64,RGB(255,0,255) ALPHA 0.13
13280   Gv=768-(GhostPose*4+GDir\16+1)*64
13290   GScreenY=GScreenY+GhostZ*ZProj
13300   TRECTANGLE Car$,GScreenX-32,GScreenY-21,GScreenX+32,GScreenY+43,Gu,Gv,Gu+64,Gv+64,RGB(255,0,255) ALPHA 0.38
13310 SUBEND
13320 ' ----------------------------------------------------------------------
13330 DEF SUB drawHud
13340   LOCAL State$, GhostState$, Progress
13350   ' Current lap, session best, saved record, last lap, speed and small map.
13360   FRECTANGLE 0,536,799,599,RGB(18,29,29) ALPHA 0.6
13370   FRECTANGLE 0,535,799,536,RGB(189,146,70)
13380   SMALLFONT TRANSPARENT
13390   INK RGB(199,172,116) : MOVE 22,586 : LABEL "DUSTLINE"
13420   INK RGB(139,155,139) : MOVE 22,565 : LABEL "LA CANTERA"
13450   INK RGB(139,155,139) : MOVE 190,587 : LABEL "CURRENT LAP"
13480   BIGFONT TRANSPARENT
13490   INK RGB(248,228,180) : MOVE 190,568 : LABEL FNCLOCK$(LapTime)
13520   SMALLFONT TRANSPARENT
13530   INK RGB(139,155,139) : MOVE 376,587 : LABEL "SESSION BEST"
13560   INK RGB(231,207,150) : MOVE 376,565 : LABEL FNCLOCK$(SessionBest)
13590   INK RGB(139,155,139) : MOVE 528,587 : LABEL "PERSONAL BEST"
13620   INK RGB(231,207,150) : MOVE 528,565 : LABEL FNCLOCK$(Best)
13650   INK RGB(139,155,139) : MOVE 684,587 : LABEL "LAST LAP"
13680   INK RGB(216,217,185) : MOVE 684,565 : LABEL FNCLOCK$(LastLap)
13710   FRECTANGLE 0,0,799,33,RGB(18,29,29) ALPHA 0.6
13720   INK RGB(173,184,162)
13730   MOVE 18,22 : LABEL "ARROWS/WASD  SPACE: HANDBRAKE  G: GHOST  P: PAUSE  R: RESTART  M: SOUND  ESC: QUIT"
13750   FRECTANGLE 20,48,178,114,RGB(18,29,29) ALPHA 0.6
13760   BIGFONT TRANSPARENT
13770   INK RGB(247,221,166) : MOVE 34,101 : LABEL DEC$(Speed*0.36,"##0")
13800   SMALLFONT TRANSPARENT
13810   INK RGB(145,163,145) : MOVE 100,89 : LABEL "KM/H"
13840   IF Air THEN State$="AIRBORNE" ELSE State$="DIRT"
13890   IF Handbrake AND Air=0 THEN State$="HANDBRAKE"
13920   IF Slip>0.3 AND Air=0 AND Handbrake=0 THEN State$="DRIFTING"
13950   MOVE 34,65 : LABEL State$
13970   TRECTANGLE MiniView$(MiniView),620,48,780,160, RGB(56,71,59) ALPHA 0.6
13980   FCIRCLE 628+Px*144/MapW,152-Py*96/WorldH,3,RGB(248,210,120)
13990   INK RGB(164,180,156) : MOVE 628,181 : LABEL "LAP "+DEC$(Laps+1,"0#")
14020   GhostState$="G: GHOST --"
14030   IF GhostN>1 THEN GhostState$="G: GHOST ON"
14060   IF GhostShow=0 THEN GhostState$="G: GHOST OFF"
14090   INK RGB(164,180,156) : MOVE 628,200 : LABEL GhostState$
14120   FRECTANGLE 222,63,580,66,RGB(56,71,59) ALPHA 0.6
14130   Progress=0
14140   IF Started THEN
14150     Progress=NextGate
14160     IF NextGate=0 THEN Progress=NGates
14190   ENDIF
14200   IF Progress>0 THEN FRECTANGLE 222,63,222+358*Progress/NGates,66,RGB(194,154,77)
14230   SMALLFONT TRANSPARENT
14240   INK RGB(144,161,142) : MOVE 222,89 : LABEL "SECTOR "+DEC$(MIN(3,INT(MAX(0,Progress-1)/4)+1),"0")+" / 3"
14270   IF SectorDone>0 THEN CALL drawSplit
14300   IF TIME<MsgT AND Phase=2 THEN CALL drawMessage
14330   IF WrongTime>0.7 AND Paused=0 THEN MsgDisplay$="WRONG WAY":CALL drawCenteredMessage
14370   IF Demo THEN
14380     INK RGB(240,190,99) : MOVE 22,511 : LABEL "DEMO  /  PRESS D TO PLAY"
14410   ENDIF
14420   IF Invalid THEN
14430     INK RGB(246,151,102) : MOVE 225,115 : LABEL "INVALID LAP - PRESS R TO RESTART"
14460   ENDIF
14470 SUBEND
14480 ' ----------------------------------------------------------------------
14490 DEF SUB drawSplit
14495   LOCAL SplitText$
14500   INK SectorColor
14510   ' Right-align every sector result with the progress bar's right edge.
14520   IF Invalid THEN
14530     SplitText$="INVALID SECTOR"
14560   ELSEIF SectorRef<=0 OR Demo THEN
14570     SplitText$="S"+DEC$(SectorDone,"0")+" NO REFERENCE"
14590   ELSE
14600     SplitText$="S"+DEC$(SectorDone,"0")+" "+DEC$(SectorDelta,"+##.000")+" s"
14602   ENDIF
14605   MOVE 580-8*LEN(SplitText$),89 : LABEL SplitText$
14610 SUBEND
14620 ' ----------------------------------------------------------------------
14630 DEF SUB drawMessage
14640   MsgDisplay$=Msg$ : CALL drawCenteredMessage
14660 SUBEND
14670 ' ----------------------------------------------------------------------
14680 DEF SUB drawCenteredMessage
14690   FRECTANGLE 158,465,641,500,RGB(18,29,29) ALPHA 0.6
14700   ' SMALLFONT is 8 by 16 pixels; centre the text inside the message panel.
14710   SMALLFONT TRANSPARENT
14720   INK RGB(241,211,147) : MOVE 400-4*LEN(MsgDisplay$),490 : LABEL MsgDisplay$
14750 SUBEND
14760 ' ----------------------------------------------------------------------
14770 DEF SUB drawTitle
14775   ' Centre headings and prompts; keep both key-table columns left-aligned.
14780   FRECTANGLE 166,173,634,449,RGB(14,25,25) ALPHA 0.9
14790   FRECTANGLE 166,447,634,450,RGB(221,166,76)
14800   INK RGB(242,217,162)
14810   BIGFONT TRANSPARENT
14820   MOVE 280,419 : LABEL "D U S T L I N E"
14840   SMALLFONT TRANSPARENT
14850   INK RGB(192,166,112) : MOVE 300,382 : LABEL "LA CANTERA  /  TIME TRIAL"
14880   INK RGB(170,189,164) : MOVE 284,345 : LABEL "One car. Dirt. Your best lap."
14910   MOVE 240,315 : LABEL "ARROWS / WASD" : MOVE 368,315 : LABEL "Accelerate, brake, steer"
14930   MOVE 240,292 : LABEL "SPACE" : MOVE 368,292 : LABEL "Handbrake"
14950   MOVE 240,269 : LABEL "P / R" : MOVE 368,269 : LABEL "Pause / restart"
14970   MOVE 240,246 : LABEL "G" : MOVE 368,246 : LABEL "Show / hide ghost"
14990   INK RGB(241,211,147) : MOVE 324,226 : LABEL "PRESS ENTER TO RACE"
15020   INK RGB(139,160,137) : MOVE 288,201 : LABEL "D: watch a demonstration lap"
15050 SUBEND
15060 ' ----------------------------------------------------------------------
15070 DEF SUB drawPause
15080   FRECTANGLE 248,258,552,352,RGB(14,25,25) ALPHA 0.6
15090   BIGFONT TRANSPARENT
15100   INK RGB(242,217,162) : MOVE 352,329 : LABEL "PAUSED"
15130   SMALLFONT TRANSPARENT
15140   MOVE 324,289 : LABEL "PRESS P TO CONTINUE"
15160 SUBEND
15170 ' ----------------------------------------------------------------------
15180 DEF SUB drawCountdown
15190   FRECTANGLE 332,273,468,367,RGB(14,25,25) ALPHA 0.6
15200   BIGFONT TRANSPARENT
15210   INK RGB(242,217,162) : MOVE 392,338 : LABEL DEC$(MAX(1,INT(ReadyUntil-TIME)+1),"0")
15240   SMALLFONT TRANSPARENT
15250   MOVE 380,297 : LABEL "READY"
15270 SUBEND
15280 ' ----------------------------------------------------------------------
15290 DEF SUB emitMud
15300   LOCAL Ca, Sa, Ax1, Ay1, Ax2, Ay2
15310   LOCAL Splat, MudSide, MudBack, Skidding
15320   Skidding=ABS(SlipR)>0.12 AND Speed>80
15330   IF Air OR Skidding=0 THEN
15340     MarkValid=0 : MarkT=0
15360     EXIT SUB
15370   ENDIF
15380   IF MarkT<1/45 THEN EXIT SUB
15410   MarkT=0
15420   Ca=COS(Ang) : Sa=SIN(Ang)
15440   RutStamp=RutStamp+1
15450   MWx=Px-13*CarScale*Ca-10.4*CarScale*Sa
15460   MWy=Py-13*CarScale*Sa+10.4*CarScale*Ca
15470   CALL projectMud
15480   Ax1=MWx : Ay1=MudY
15500   MWx=Px-13*CarScale*Ca+10.4*CarScale*Sa
15510   MWy=Py-13*CarScale*Sa-10.4*CarScale*Ca
15520   CALL projectMud
15530   Ax2=MWx : Ay2=MudY
15550   IF MarkValid=0 THEN
15560     Last1X=Ax1 : Last1Y=Ay1
15580     Last2X=Ax2 : Last2Y=Ay2
15600     MarkValid=1
15610     EXIT SUB
15620   ENDIF
15630   ' Only sliding tyres disturb soil; repeated crossings remix existing patches.
15640   RutCut=0.1+MIN(0.25,ABS(SlipR)*0.45)+Handbrake*0.08
15650   RutX0=Last1X : RutY0=Last1Y : RutX1=Ax1 : RutY1=Ay1
15690   CALL stampRut
15700   RutX0=Last2X : RutY0=Last2Y : RutX1=Ax2 : RutY1=Ay2
15740   CALL stampRut
15750   ' Broad sideways clods also settle over arrows, the wood deck and kerbs.
15760   FOR Splat=1 TO 2
15770     MudSide=(RND-0.5)*(36+MIN(1,ABS(SlipR))*85)
15780     MudBack=14+RND*27
15790     MWx=Px-MudBack*Ca-MudSide*Sa
15800     MWy=Py-MudBack*Sa+MudSide*Ca
15810     CALL projectMud
15820     RutX0=MWx : RutY0=MudY : RutX1=MWx+1.5 : RutY1=MudY+0.5
15860     CALL stampRut
15870   NEXT Splat
15880   Last1X=Ax1 : Last1Y=Ay1
15900   Last2X=Ax2 : Last2Y=Ay2
15920 SUBEND
15930 ' Individual sector times are read through an array reference, without copying it.
15940 DEF FNSECTORDURATION(Times, Sector)
15950   FNSECTORDURATION=Times(Sector)-Times(Sector-1)
15960 FNEND
15970 DEF FNA2(Y,X)
15980   LOCAL A
15990   IF ABS(X)<0.0000001 THEN FNA2=SGN(Y)*PI/2:EXIT FN
16030   A=ATN(Y/X)
16040   IF X<0 THEN A=A+PI
16070   FNA2=A
16080 FNEND
16090 DEF FNWRAP(A)=A-2*PI*INT((A+PI)/(2*PI))
16100 DEF FNCLOCK$(S)
16110   LOCAL M,T
16120   IF S<=0 THEN FNCLOCK$="--:--.---":EXIT FN
16160   M=INT(S/60)
16170   T=INT((S-M*60)*1000)
16180   FNCLOCK$=DEC$(M,"00")+":"+DEC$(INT(T/1000),"00")+"."+DEC$(T MOD 1000,"000")
16190 FNEND
