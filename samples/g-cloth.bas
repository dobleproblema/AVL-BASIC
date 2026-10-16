10 ' Watch satin cloth fall and drape over a sphere.
20 ' SPACE pauses; N advances one simulation step while paused.
30 ' Drag with the mouse while running; arrow keys orbit; +/- zoom.
40 ' S: shadows; B: sphere motion; W: wind; M: material.
50 ' R restarts the fall; ESC exits.
60 SCREEN : MODE 800 : RAD : PAPER 0 : CLG : MAT BASE 0
70 WidthPx=WIDTH : HeightPx=HEIGHT : CentreX=WidthPx*.5 : CentreY=HeightPx*.49 : FocalLength=WidthPx*1.04
80 ' CellsX and CellsY count cloth cells; PhysicsHz sets simulation steps per second.
90 ' Uncomment the smaller-sheet configuration below for fewer particles.
100 ClothWidth=4.6 : ClothDepth=3.7
110 CellsX=24 : CellsY=16 : PhysicsHz=10
120 IF PhysicsHz<=0 THEN PRINT "PhysicsHz must be positive." : SCREEN CLOSE : END
130 StepDt=1/PhysicsHz : BatchSteps=3
140 ' BatchSteps sets the number of physical substeps in each timing batch.
150 ' CellsX=16 : CellsY=12 : ClothWidth=3.68 : ClothDepth=2.775
160 XpbdIters=8 : RelaxIters=12 : SelfContact=1 : FloorContact=0
170 ' Self-contact separates nearby cloth particles.
180 SelfSeparation=.95 : SelfNeighborSkip=1 : SelfFinalPasses=2
190 CollisionThickness=.025
200 ' SelfSeparation is a fraction of cell spacing; SelfFinalPasses adds contact sweeps.
210 SphereSegments=20 : SphereRings=14 : Shadows=1 : ShadowSize=128 : ShadowPcf=3
211 ' Higher shadow quality: ShadowSize=256 : ShadowPcf=5.
220 ' SphereSegments and SphereRings set the sphere tessellation.
230 ' FloorContact=1 enables an invisible horizontal collider at Y=.035.
240 BallX=0 : BallY=1.45 : BallZ=-.25 : BallRadius=.9
250 Azimuth=.48 : Elevation=.45 : Distance=7.2 : TargetY=1.6
260 Scene=1 : PinMode=0 : Wind=1 : Material=0 : BallMove=0
270 ShowHud=1 : SettleSteps=0
280 MaxCatchup=4 : Interpolate=1 : DemoSeconds=0
290 ' BenchMode=1 enables timing; BenchPhase: 0=both, 1=physics, 2=drawing.
300 BenchMode=0 : BenchPhase=0 : BenchFrames=5 : BenchWarmup=1
310 StateIn$="" : StateOut$="" : FinalPng$=""
320 ' SettleSteps advances the initial simulation; DemoSeconds sets an optional time limit.
330 IF CellsX<2 OR CellsY<2 OR CellsX<>INT(CellsX) OR CellsY<>INT(CellsY) THEN PRINT "Invalid cloth grid." : SCREEN CLOSE : END
340 IF ClothWidth<=0 OR ClothDepth<=0 THEN PRINT "Cloth dimensions must be positive." : SCREEN CLOSE : END
350 IF StepDt<=0 OR BatchSteps<1 OR BatchSteps<>INT(BatchSteps) THEN PRINT "Invalid fixed physical step." : SCREEN CLOSE : END
360 IF SelfSeparation<=0 OR SelfNeighborSkip<1 OR SelfNeighborSkip<>INT(SelfNeighborSkip) OR SelfFinalPasses<0 OR SelfFinalPasses<>INT(SelfFinalPasses) THEN PRINT "Invalid self contact settings." : SCREEN CLOSE : END
370 IF XpbdIters<1 OR RelaxIters<1 OR XpbdIters<>INT(XpbdIters) OR RelaxIters<>INT(RelaxIters) THEN PRINT "Invalid solver sweeps." : SCREEN CLOSE : END
380 IF CollisionThickness<0 THEN PRINT "Invalid collision thickness." : SCREEN CLOSE : END
390 IF SphereSegments<3 OR SphereRings<2 OR SphereSegments<>INT(SphereSegments) OR SphereRings<>INT(SphereRings) THEN PRINT "Invalid sphere tessellation." : SCREEN CLOSE : END
391 IF ShadowSize<16 OR ShadowSize>1024 OR ShadowSize<>INT(ShadowSize) THEN PRINT "Invalid shadow map size." : SCREEN CLOSE : END
400 IF MaxCatchup<1 OR MaxCatchup<>INT(MaxCatchup) OR DemoSeconds<0 THEN PRINT "Invalid realtime clock settings." : SCREEN CLOSE : END
410 IF SettleSteps<0 OR SettleSteps<>INT(SettleSteps) THEN PRINT "Invalid settling steps." : SCREEN CLOSE : END
420 IF BenchFrames<0 OR BenchFrames<>INT(BenchFrames) OR BenchWarmup<0 OR BenchWarmup<>INT(BenchWarmup) OR BenchPhase<0 OR BenchPhase>2 THEN PRINT "Invalid benchmark settings." : SCREEN CLOSE : END
430 BatchHz=1/(StepDt*BatchSteps) : SimTime=0 : FrameIndex=0 : Grab=-1 : LastLeft=0
440 BallStartTime=0 : BallStartX=BallX : BallStartY=BallY : BallStartZ=BallZ : CALL initializeCloth
450 IF StateIn$<>"" THEN CALL loadCheckpoint(PosX,PosY,PosZ,VelocityX,VelocityY,VelocityZ)
460 ' The solver and optional checkpoint are ready; render setup follows.
470 CALL initializeMesh : CALL initializeLights
480 IF StateIn$="" THEN
490   FOR Settle=1 TO SettleSteps
500     SimTime=SimTime+StepDt : CALL stepCloth
510   NEXT Settle
520   BallStartTime=SimTime
530 ENDIF
540 CALL cacheDrawState : RenderAlpha=1
550 CALL drawScene : IF ShowHud AND BenchMode=0 THEN CALL drawHud
560 FRAME
570 IF BenchMode=0 THEN
580   Paused=0 : SingleStep=0 : Done=0 : Fps=0 : FpsCount=0 : Accumulator=0 : DroppedTime=0 : LiveFrames=0 : PhysicsSteps=0
590   PRINT "CLOTH_LIVE_BEGIN ";SimTime;" ";StepDt;" ";CellsX;" ";CellsY
600   LiveStart=TIME : LastWall=LiveStart : FpsLast=LiveStart
610   WHILE 1
620     Now=TIME : WallDt=MAX(0,Now-LastWall) : LastWall=Now : InputDt=MIN(.1,WallDt) : CALL readControls
630     IF DemoSeconds>0 AND Now-LiveStart>=DemoSeconds THEN Done=1
640     IF Done THEN EXIT WHILE
650     IF Paused THEN
660       Accumulator=0 : RenderAlpha=1
670       IF SingleStep THEN CALL advanceStep
680       SingleStep=0
690     ELSE
700       Accumulator=Accumulator+WallDt : CALL catchUp : RenderAlpha=MAX(0,MIN(1,Accumulator/StepDt))
710     ENDIF
720     CALL drawScene : IF ShowHud THEN CALL drawHud
730     FRAME
740     LiveFrames=LiveFrames+1 : FpsCount=FpsCount+1 : Now=TIME : IF Now-FpsLast>=.5 THEN Fps=FpsCount/(Now-FpsLast):FpsCount=0:FpsLast=Now
750   WEND
760 ELSE
770   ' Timings exclude initialization, initial render, final exports and warmup markers.
780   IF BenchFrames>0 THEN
790     FOR BenchIndex=1-BenchWarmup TO BenchFrames
800       IF BenchIndex>0 THEN PRINT "CLOTH_FRAME_BEGIN ";BenchIndex;" ";SimTime
810       IF BenchPhase<>2 THEN
820         IF BenchIndex>0 THEN PRINT "CLOTH_PH_BEGIN ";BenchIndex;" ";SimTime
830         CALL advanceBatch
840         IF BenchIndex>0 THEN PRINT "CLOTH_PH_DONE ";BenchIndex;" ";SimTime
850       ENDIF
860       IF BenchPhase<>1 THEN
870         CALL drawScene : FRAME
880         IF BenchIndex>0 THEN PRINT "CLOTH_RENDER_DONE ";BenchIndex;" ";SimTime
890       ENDIF
900       IF BenchIndex>0 THEN PRINT "CLOTH_FRAME_DONE ";BenchIndex;" ";SimTime
910     NEXT BenchIndex
920   ENDIF
930 ENDIF
940 ' Save the final image and physical checkpoint, if requested.
950 IF BenchMode=0 THEN LiveSeconds=TIME-LiveStart:PRINT "CLOTH_LIVE_DONE ";LiveFrames;" ";PhysicsSteps;" ";SimTime;" ";LiveSeconds;" ";DroppedTime
960 IF BenchMode=0 THEN PRINT "CLOTH_DRAW_TIME ";DrawTime
970 IF BenchMode AND BenchPhase=1 THEN CALL drawScene : FRAME
980 IF FinalPng$<>"" THEN BSAVE FinalPng$
990 IF StateOut$<>"" THEN CALL saveCheckpoint(PosX,PosY,PosZ,VelocityX,VelocityY,VelocityZ)
1000 IF BenchMode THEN PRINT "CLOTH_BENCH_DONE ";FrameIndex;" ";SimTime
1010 SCREEN CLOSE : END
1020 ' ----------------------------------------------------------------------
1030 DEF SUB initializeCloth
1040   LOCAL GridRow, GridCol, V, U, Particle, A, B, C, RestLength, Compliance
1050   LOCAL BendFactor, TriangleMass
1060   ' Cloth solver: area masses, XPBD membrane, curvature bending, stretch limits.
1070   ' Shared simulation state: positions, velocities, topology, clocks and sphere history.
1080   ParticleStride=CellsX+1 : ParticleCount=ParticleStride*(CellsY+1) : CellWidth=ClothWidth/CellsX : CellDepth=ClothDepth/CellsY
1090   IF StepDt=0 THEN StepDt=1/180
1100   AreaDensity=0.075 : StretchCompliance=0.00002 : ShearCompliance=0.00010 : BendRigidity=0.00003
1110   SolverIters=XpbdIters : ContactThickness=CollisionThickness : Friction=0.28 : SelfEnabled=SelfContact : SelfDistance=MIN(CellWidth,CellDepth)*SelfSeparation
1120   PhysicsTime=0 : BallPrevX=BallX : BallPrevY=BallY : BallPrevZ=BallZ
1130   DIM PosX(ParticleCount-1),PosY(ParticleCount-1),PosZ(ParticleCount-1),OldX(ParticleCount-1),OldY(ParticleCount-1),OldZ(ParticleCount-1),InvMass(ParticleCount-1)
1140   DIM NormalX(ParticleCount-1),NormalY(ParticleCount-1),NormalZ(ParticleCount-1),Mass(ParticleCount-1),BaseInvMass(ParticleCount-1)
1150   DIM VelocityX(ParticleCount-1),VelocityY(ParticleCount-1),VelocityZ(ParticleCount-1),ForceX(ParticleCount-1),ForceY(ParticleCount-1),ForceZ(ParticleCount-1)
1160   DIM ContactDepth(ParticleCount-1),ContactType(ParticleCount-1),ContactNx(ParticleCount-1),ContactNy(ParticleCount-1),ContactNz(ParticleCount-1)
1170   MaxEdges=6*ParticleCount : DIM EdgeA(MaxEdges-1),EdgeB(MaxEdges-1),EdgeLength(MaxEdges-1),EdgeCompliance(MaxEdges-1),EdgeLambda(MaxEdges-1)
1180   MaxBends=2*ParticleCount : DIM BendA(MaxBends-1),BendB(MaxBends-1),BendC(MaxBends-1),BendScale(MaxBends-1)
1190   DIM BendLambdaX(MaxBends-1),BendLambdaY(MaxBends-1),BendLambdaZ(MaxBends-1)
1200   MaxTriangles=2*CellsX*CellsY : DIM TriangleA(MaxTriangles-1),TriangleB(MaxTriangles-1),TriangleC(MaxTriangles-1)
1210   HashSize=4099 : DIM HashHead(HashSize-1),HashNext(ParticleCount-1),HashX(ParticleCount-1),HashY(ParticleCount-1),HashZ(ParticleCount-1)
1220   EdgeCount=0 : BendCount=0 : TriangleCount=0 : MAT Mass=0
1230   DIM TetherLeft(ParticleCount-1),TetherRight(ParticleCount-1)
1240   FOR GridRow=0 TO CellsY
1250     V=GridRow/CellsY
1260     FOR GridCol=0 TO CellsX
1270       U=GridCol/CellsX : Particle=GridRow*ParticleStride+GridCol
1280       TetherLeft(Particle)=1.01*SQR((GridCol*CellWidth)^2+(GridRow*CellDepth)^2) : TetherRight(Particle)=1.01*SQR(((CellsX-GridCol)*CellWidth)^2+(GridRow*CellDepth)^2)
1290       PosX(Particle)=-1.6+3.2*U : PosY(Particle)=3.9-3.4*V
1300       PosZ(Particle)=0.34*SIN(4*PI*U)+1.1*SIN(PI*V/2)
1310       IF Scene=1 THEN PosX(Particle)=ClothWidth*(U-.5):PosY(Particle)=3.02+.015*SIN(2*PI*U)*SIN(2*PI*V):PosZ(Particle)=ClothDepth*(V-.5)-.25
1320       IF GridCol<CellsX THEN A=Particle:B=Particle+1:RestLength=CellWidth:Compliance=StretchCompliance:CALL addEdge
1330       IF GridRow<CellsY THEN A=Particle:B=Particle+ParticleStride:RestLength=CellDepth:Compliance=StretchCompliance:CALL addEdge
1340       IF GridCol<CellsX AND GridRow<CellsY THEN CALL addCellTriangles
1350       IF GridCol>0 AND GridCol<CellsX THEN A=Particle-1:B=Particle:C=Particle+1:BendFactor=SQR(CellWidth*CellDepth)/(CellWidth*CellWidth):CALL addBend
1360       IF GridRow>0 AND GridRow<CellsY THEN A=Particle-ParticleStride:B=Particle:C=Particle+ParticleStride:BendFactor=SQR(CellWidth*CellDepth)/(CellDepth*CellDepth):CALL addBend
1370     NEXT GridCol
1380   NEXT GridRow
1390   FOR Particle=0 TO ParticleCount-1
1400     BaseInvMass(Particle)=1/Mass(Particle)
1410   NEXT Particle
1420   MAT InvMass=BaseInvMass : IF PinMode>=1 THEN InvMass(0)=0
1430   IF PinMode=2 THEN InvMass(CellsX)=0
1440   IF Grab>=0 AND Grab<ParticleCount THEN InvMass(Grab)=0
1450   CALL projectContacts
1460   MAT OldX=PosX : MAT OldY=PosY : MAT OldZ=PosZ : MAT VelocityX=0 : MAT VelocityY=0 : MAT VelocityZ=0
1470   CALL updatePhysicalNormals
1480 SUBEND
1490 ' ----------------------------------------------------------------------
1500 DEF SUB addCellTriangles
1510   ' Both cloth diagonals resist shear; triangles face the positive-Z side.
1520   RestLength=SQR(CellWidth*CellWidth+CellDepth*CellDepth) : Compliance=ShearCompliance
1530   A=Particle : B=Particle+ParticleStride+1 : CALL addEdge
1540   A=Particle+1 : B=Particle+ParticleStride : CALL addEdge
1550   A=Particle : B=Particle+ParticleStride+1 : C=Particle+1 : CALL addTriangle
1560   A=Particle : B=Particle+ParticleStride : C=Particle+ParticleStride+1 : CALL addTriangle
1570 SUBEND
1580 ' ----------------------------------------------------------------------
1590 DEF SUB addEdge
1600   EdgeA(EdgeCount)=A : EdgeB(EdgeCount)=B : EdgeLength(EdgeCount)=RestLength : EdgeCompliance(EdgeCount)=Compliance : EdgeCount=EdgeCount+1
1610 SUBEND
1620 ' ----------------------------------------------------------------------
1630 DEF SUB addBend
1640   BendA(BendCount)=A : BendB(BendCount)=B : BendC(BendCount)=C : BendScale(BendCount)=BendFactor : BendCount=BendCount+1
1650 SUBEND
1660 ' ----------------------------------------------------------------------
1670 DEF SUB addTriangle
1680   TriangleA(TriangleCount)=A : TriangleB(TriangleCount)=B : TriangleC(TriangleCount)=C : TriangleCount=TriangleCount+1
1690   TriangleMass=AreaDensity*CellWidth*CellDepth/6
1700   Mass(A)=Mass(A)+TriangleMass : Mass(B)=Mass(B)+TriangleMass : Mass(C)=Mass(C)+TriangleMass
1710 SUBEND
1720 ' ----------------------------------------------------------------------
1730 DEF SUB stepCloth
1740   LOCAL Particle, Iteration, LimitIteration, FinalPass
1750   ' One fixed physical substep, independent of rendering rate.
1760   StepDt2=StepDt*StepDt : Damping=EXP(-0.18*StepDt) : PhysicsTime=PhysicsTime+StepDt
1770   MAT InvMass=BaseInvMass
1780   IF PinMode>=1 THEN InvMass(0)=0:PosX(0)=-1.6:PosY(0)=3.9:PosZ(0)=0
1790   IF PinMode=2 THEN InvMass(CellsX)=0:PosX(CellsX)=1.6:PosY(CellsX)=3.9:PosZ(CellsX)=0
1800   IF Grab>=0 AND Grab<ParticleCount THEN InvMass(Grab)=0
1810   CALL projectGrab
1820   BallDeltaX=BallX-BallPrevX : BallDeltaY=BallY-BallPrevY : BallDeltaZ=BallZ-BallPrevZ
1830   MAT OldX=PosX : MAT OldY=PosY : MAT OldZ=PosZ
1840   MAT ForceX=0 : MAT ForceY=0 : MAT ForceZ=0 : CALL applyAirForces
1850   FOR Particle=0 TO ParticleCount-1
1860     IF InvMass(Particle)=0 THEN
1870       VelocityX(Particle)=0 : VelocityY(Particle)=0 : VelocityZ(Particle)=0
1880     ELSE
1890       VelocityX(Particle)=(VelocityX(Particle)+StepDt*InvMass(Particle)*ForceX(Particle))*Damping
1900       VelocityY(Particle)=(VelocityY(Particle)+StepDt*(InvMass(Particle)*ForceY(Particle)-9.81))*Damping
1910       VelocityZ(Particle)=(VelocityZ(Particle)+StepDt*InvMass(Particle)*ForceZ(Particle))*Damping
1920       PosX(Particle)=PosX(Particle)+StepDt*VelocityX(Particle) : PosY(Particle)=PosY(Particle)+StepDt*VelocityY(Particle) : PosZ(Particle)=PosZ(Particle)+StepDt*VelocityZ(Particle)
1930     ENDIF
1940   NEXT Particle
1950   MAT EdgeLambda=0 : MAT BendLambdaX=0 : MAT BendLambdaY=0 : MAT BendLambdaZ=0
1960   MAT ContactDepth=0 : MAT ContactType=0
1970   FOR Iteration=1 TO SolverIters
1980     CALL solveDistances : CALL solveBending
1990     CALL solveTethers : CALL limitStrain
2000     IF SelfEnabled AND Iteration>=SolverIters-1 THEN CALL solveSelfContact
2010     CALL projectContacts
2020   NEXT Iteration
2030   CALL applyFriction : CALL projectContacts
2040   FOR LimitIteration=1 TO RelaxIters
2050     Iteration=LimitIteration : CALL solveTethers : CALL limitStrain : CALL projectContacts
2060   NEXT LimitIteration
2070   IF SelfEnabled THEN
2080     FOR FinalPass=1 TO SelfFinalPasses
2090       CALL solveSelfContact : CALL projectContacts
2100     NEXT FinalPass
2110   ENDIF
2120   MAT VelocityX=(PosX-OldX)/StepDt : MAT VelocityY=(PosY-OldY)/StepDt : MAT VelocityZ=(PosZ-OldZ)/StepDt
2130   ' Only the pins and the current grab have zero inverse mass.
2140   IF PinMode>=1 THEN VelocityX(0)=0:VelocityY(0)=0:VelocityZ(0)=0
2141   IF PinMode=2 THEN VelocityX(CellsX)=0:VelocityY(CellsX)=0:VelocityZ(CellsX)=0
2142   IF Grab>=0 AND Grab<ParticleCount THEN VelocityX(Grab)=0:VelocityY(Grab)=0:VelocityZ(Grab)=0
2160   BallPrevX=BallX : BallPrevY=BallY : BallPrevZ=BallZ
2170 SUBEND
2180 ' ----------------------------------------------------------------------
2190 DEF SUB projectGrab
2200   LOCAL GrabDx, GrabDy, GrabDz, GrabLength, GrabScale
2210   ' A held vertex respects the moving sphere and optional invisible floor.
2220   IF Grab<0 OR Grab>=ParticleCount THEN EXIT SUB
2230   GrabDx=PosX(Grab)-BallX : GrabDy=PosY(Grab)-BallY : GrabDz=PosZ(Grab)-BallZ : GrabLength=SQR(GrabDx^2+GrabDy^2+GrabDz^2)
2240   IF GrabLength<.00000001 THEN GrabDx=0:GrabDy=0:GrabDz=.00000001:GrabLength=.00000001
2250   IF GrabLength<BallRadius+ContactThickness THEN GrabScale=(BallRadius+ContactThickness)/GrabLength:PosX(Grab)=BallX+GrabDx*GrabScale:PosY(Grab)=BallY+GrabDy*GrabScale:PosZ(Grab)=BallZ+GrabDz*GrabScale
2260   IF FloorContact THEN PosY(Grab)=MAX(.035+ContactThickness,PosY(Grab))
2270 SUBEND
2280 ' ----------------------------------------------------------------------
2290 DEF SUB solveDistances
2300   LOCAL SweepStart, SweepStop, SweepStep, Constraint, A, B, WeightA, WeightB, DeltaX, DeltaY
2310   LOCAL DeltaZ, Length, ComplianceScale, DeltaLambda
2320   ' Hot loops use early-out tail skips; AVL BASIC has no CONTINUE statement.
2330   ' XPBD distance constraints; reverse sweeps reduce directional bias.
2340   SweepStart=0 : SweepStop=EdgeCount-1 : SweepStep=1
2350   IF Iteration MOD 2=0 THEN SweepStart=EdgeCount-1:SweepStop=0:SweepStep=-1
2360   FOR Constraint=SweepStart TO SweepStop STEP SweepStep
2370     A=EdgeA(Constraint) : B=EdgeB(Constraint) : WeightA=InvMass(A) : WeightB=InvMass(B)
2380     IF WeightA+WeightB=0 THEN GOTO 2450
2390     DeltaX=PosX(A)-PosX(B) : DeltaY=PosY(A)-PosY(B) : DeltaZ=PosZ(A)-PosZ(B)
2400     Length=SQR(DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ) : IF Length<0.00000001 THEN GOTO 2450
2410     ComplianceScale=EdgeCompliance(Constraint)/StepDt2 : DeltaLambda=(-(Length-EdgeLength(Constraint))-ComplianceScale*EdgeLambda(Constraint))/(WeightA+WeightB+ComplianceScale)
2420     EdgeLambda(Constraint)=EdgeLambda(Constraint)+DeltaLambda : DeltaX=DeltaX/Length : DeltaY=DeltaY/Length : DeltaZ=DeltaZ/Length
2430     PosX(A)=PosX(A)+WeightA*DeltaLambda*DeltaX : PosY(A)=PosY(A)+WeightA*DeltaLambda*DeltaY : PosZ(A)=PosZ(A)+WeightA*DeltaLambda*DeltaZ
2440     PosX(B)=PosX(B)-WeightB*DeltaLambda*DeltaX : PosY(B)=PosY(B)-WeightB*DeltaLambda*DeltaY : PosZ(B)=PosZ(B)-WeightB*DeltaLambda*DeltaZ
2450   NEXT Constraint
2460 SUBEND
2470 ' ----------------------------------------------------------------------
2480 DEF SUB solveBending
2490   LOCAL ComplianceScale, SweepStart, SweepStop, SweepStep, Constraint, A, B, C, BendFactor, WeightA
2500   LOCAL WeightB, WeightC, Denominator, DeltaLambdaX, DeltaLambdaY, DeltaLambdaZ
2510   ' Cvec=sqrt(dualarea)/step^2*(xa-2xb+xc); isotropic quadratic bending.
2520   ComplianceScale=1/(BendRigidity*StepDt2) : SweepStart=0 : SweepStop=BendCount-1 : SweepStep=1
2530   IF Iteration MOD 2=0 THEN SweepStart=BendCount-1:SweepStop=0:SweepStep=-1
2540   FOR Constraint=SweepStart TO SweepStop STEP SweepStep
2550     A=BendA(Constraint) : B=BendB(Constraint) : C=BendC(Constraint) : BendFactor=BendScale(Constraint)
2560     WeightA=InvMass(A) : WeightB=InvMass(B) : WeightC=InvMass(C)
2570     Denominator=BendFactor*BendFactor*(WeightA+4*WeightB+WeightC)+ComplianceScale
2580     DeltaLambdaX=(-BendFactor*(PosX(A)-2*PosX(B)+PosX(C))-ComplianceScale*BendLambdaX(Constraint))/Denominator
2590     DeltaLambdaY=(-BendFactor*(PosY(A)-2*PosY(B)+PosY(C))-ComplianceScale*BendLambdaY(Constraint))/Denominator
2600     DeltaLambdaZ=(-BendFactor*(PosZ(A)-2*PosZ(B)+PosZ(C))-ComplianceScale*BendLambdaZ(Constraint))/Denominator
2610     BendLambdaX(Constraint)=BendLambdaX(Constraint)+DeltaLambdaX : BendLambdaY(Constraint)=BendLambdaY(Constraint)+DeltaLambdaY : BendLambdaZ(Constraint)=BendLambdaZ(Constraint)+DeltaLambdaZ
2620     WeightA=WeightA*BendFactor : WeightB=-2*WeightB*BendFactor : WeightC=WeightC*BendFactor
2630     PosX(A)=PosX(A)+WeightA*DeltaLambdaX : PosY(A)=PosY(A)+WeightA*DeltaLambdaY : PosZ(A)=PosZ(A)+WeightA*DeltaLambdaZ
2640     PosX(B)=PosX(B)+WeightB*DeltaLambdaX : PosY(B)=PosY(B)+WeightB*DeltaLambdaY : PosZ(B)=PosZ(B)+WeightB*DeltaLambdaZ
2650     PosX(C)=PosX(C)+WeightC*DeltaLambdaX : PosY(C)=PosY(C)+WeightC*DeltaLambdaY : PosZ(C)=PosZ(C)+WeightC*DeltaLambdaZ
2660   NEXT Constraint
2670 SUBEND
2680 ' ----------------------------------------------------------------------
2690 DEF SUB solveTethers
2700   LOCAL Particle, Anchor, MaxLength, DeltaX, DeltaY, DeltaZ, Length2, Length, WeightA, WeightB
2710   LOCAL Correction
2720   ' Long-range unilateral attachment bounds, active only for hanging pins.
2730   IF Scene<>0 OR PinMode<1 THEN EXIT SUB
2740   FOR Particle=0 TO ParticleCount-1
2750     IF InvMass(Particle)<>0 THEN
2760       Anchor=0 : MaxLength=TetherLeft(Particle) : CALL limitTether
2770       IF PinMode=2 THEN Anchor=CellsX:MaxLength=TetherRight(Particle):CALL limitTether
2780     ENDIF
2790   NEXT Particle
2800   EXIT SUB
2810 SUBEND
2820 ' ----------------------------------------------------------------------
2830 DEF SUB limitStrain
2840   LOCAL SweepStart, SweepStop, SweepStep, Constraint, A, B, WeightA, WeightB, DeltaX, DeltaY
2850   LOCAL DeltaZ, MaxLength, Length2, Length, Correction
2860   ' Hard unilateral one-percent tensile strain bounds, structure and shear.
2870   SweepStart=0 : SweepStop=EdgeCount-1 : SweepStep=1
2880   IF Iteration MOD 2=0 THEN SweepStart=EdgeCount-1:SweepStop=0:SweepStep=-1
2890   FOR Constraint=SweepStart TO SweepStop STEP SweepStep
2900     A=EdgeA(Constraint):B=EdgeB(Constraint):WeightA=InvMass(A):WeightB=InvMass(B):IF WeightA+WeightB=0 THEN GOTO 2960
2910     DeltaX=PosX(A)-PosX(B) : DeltaY=PosY(A)-PosY(B) : DeltaZ=PosZ(A)-PosZ(B) : MaxLength=EdgeLength(Constraint)*1.01
2920     Length2=DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ:IF Length2<=MaxLength*MaxLength THEN GOTO 2960
2930     Length=SQR(Length2) : Correction=(MaxLength-Length)/((WeightA+WeightB)*Length)
2940     PosX(A)=PosX(A)+WeightA*Correction*DeltaX : PosY(A)=PosY(A)+WeightA*Correction*DeltaY : PosZ(A)=PosZ(A)+WeightA*Correction*DeltaZ
2950     PosX(B)=PosX(B)-WeightB*Correction*DeltaX : PosY(B)=PosY(B)-WeightB*Correction*DeltaY : PosZ(B)=PosZ(B)-WeightB*Correction*DeltaZ
2960   NEXT Constraint
2970 SUBEND
2980 ' ----------------------------------------------------------------------
2990 DEF SUB limitTether
3000   DeltaX=PosX(Particle)-PosX(Anchor) : DeltaY=PosY(Particle)-PosY(Anchor) : DeltaZ=PosZ(Particle)-PosZ(Anchor)
3010   Length2=DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ:IF Length2<=MaxLength*MaxLength THEN EXIT SUB
3020   Length=SQR(Length2):WeightA=InvMass(Particle):WeightB=InvMass(Anchor):IF WeightA+WeightB=0 THEN EXIT SUB
3030   Correction=(MaxLength-Length)/((WeightA+WeightB)*Length)
3040   PosX(Particle)=PosX(Particle)+WeightA*Correction*DeltaX : PosY(Particle)=PosY(Particle)+WeightA*Correction*DeltaY : PosZ(Particle)=PosZ(Particle)+WeightA*Correction*DeltaZ
3050   PosX(Anchor)=PosX(Anchor)-WeightB*Correction*DeltaX : PosY(Anchor)=PosY(Anchor)-WeightB*Correction*DeltaY : PosZ(Anchor)=PosZ(Anchor)-WeightB*Correction*DeltaZ
3060 SUBEND
3070 ' ----------------------------------------------------------------------
3080 DEF SUB projectContacts
3090   LOCAL Particle, DeltaX, DeltaY, DeltaZ, Length, Correction
3100   ' Sphere and optional floor contacts with Coulomb friction.
3110   FOR Particle=0 TO ParticleCount-1
3120     IF InvMass(Particle)=0 THEN GOTO 3240
3130     DeltaX=PosX(Particle)-BallX : DeltaY=PosY(Particle)-BallY : DeltaZ=PosZ(Particle)-BallZ
3140     Length=SQR(DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ)
3150     IF Length>=BallRadius+ContactThickness THEN GOTO 3210
3160     IF Length<0.00000001 THEN DeltaX=0:DeltaY=0:DeltaZ=1:Length=1
3170     DeltaX=DeltaX/Length : DeltaY=DeltaY/Length : DeltaZ=DeltaZ/Length
3180     Correction=BallRadius+ContactThickness-SQR((PosX(Particle)-BallX)^2+(PosY(Particle)-BallY)^2+(PosZ(Particle)-BallZ)^2)
3190     PosX(Particle)=BallX+DeltaX*(BallRadius+ContactThickness) : PosY(Particle)=BallY+DeltaY*(BallRadius+ContactThickness) : PosZ(Particle)=BallZ+DeltaZ*(BallRadius+ContactThickness)
3200     ContactDepth(Particle)=ContactDepth(Particle)+Correction : ContactType(Particle)=1 : ContactNx(Particle)=DeltaX : ContactNy(Particle)=DeltaY : ContactNz(Particle)=DeltaZ
3210     IF FloorContact=0 OR PosY(Particle)>=0.035+ContactThickness THEN GOTO 3240
3220     ContactDepth(Particle)=ContactDepth(Particle)+0.035+ContactThickness-PosY(Particle) : PosY(Particle)=0.035+ContactThickness
3230     ContactType(Particle)=2 : ContactNx(Particle)=0 : ContactNy(Particle)=1 : ContactNz(Particle)=0
3240   NEXT Particle
3250 SUBEND
3260 ' ----------------------------------------------------------------------
3270 DEF SUB applyFriction
3280   LOCAL Particle, DeltaX, DeltaY, DeltaZ, NormalDelta, Length, Correction
3290   ' One Coulomb-bounded tangential correction relative to the moving collider.
3300   FOR Particle=0 TO ParticleCount-1
3310     IF InvMass(Particle)=0 OR ContactDepth(Particle)<=0 THEN GOTO 3390
3320     DeltaX=PosX(Particle)-OldX(Particle) : DeltaY=PosY(Particle)-OldY(Particle) : DeltaZ=PosZ(Particle)-OldZ(Particle)
3330     IF ContactType(Particle)=1 THEN DeltaX=DeltaX-BallDeltaX:DeltaY=DeltaY-BallDeltaY:DeltaZ=DeltaZ-BallDeltaZ
3340     NormalDelta=DeltaX*ContactNx(Particle)+DeltaY*ContactNy(Particle)+DeltaZ*ContactNz(Particle)
3350     DeltaX=DeltaX-NormalDelta*ContactNx(Particle) : DeltaY=DeltaY-NormalDelta*ContactNy(Particle) : DeltaZ=DeltaZ-NormalDelta*ContactNz(Particle)
3360     Length=SQR(DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ):IF Length<0.00000001 THEN GOTO 3390
3370     Correction=MIN(1,Friction*ContactDepth(Particle)/Length)
3380     PosX(Particle)=PosX(Particle)-Correction*DeltaX : PosY(Particle)=PosY(Particle)-Correction*DeltaY : PosZ(Particle)=PosZ(Particle)-Correction*DeltaZ
3390   NEXT Particle
3400 SUBEND
3410 ' ----------------------------------------------------------------------
3420 DEF SUB updatePhysicalNormals
3430   LOCAL Constraint, Particle, A, B, C, Length, EdgeAx, EdgeAy, EdgeAz, EdgeBx
3440   LOCAL EdgeBy, EdgeBz, CrossX, CrossY, CrossZ, CrossLength
3450   ' Compute area-weighted vertex normals.
3460   MAT NormalX=0 : MAT NormalY=0 : MAT NormalZ=0
3470   FOR Constraint=0 TO TriangleCount-1
3480     A=TriangleA(Constraint) : B=TriangleB(Constraint) : C=TriangleC(Constraint) : CALL triangleCross
3490     NormalX(A)=NormalX(A)+CrossX : NormalY(A)=NormalY(A)+CrossY : NormalZ(A)=NormalZ(A)+CrossZ
3500     NormalX(B)=NormalX(B)+CrossX : NormalY(B)=NormalY(B)+CrossY : NormalZ(B)=NormalZ(B)+CrossZ
3510     NormalX(C)=NormalX(C)+CrossX : NormalY(C)=NormalY(C)+CrossY : NormalZ(C)=NormalZ(C)+CrossZ
3520   NEXT Constraint
3530   FOR Particle=0 TO ParticleCount-1
3540     Length=SQR(NormalX(Particle)^2+NormalY(Particle)^2+NormalZ(Particle)^2)
3550     IF Length<0.00000001 THEN NormalX(Particle)=0:NormalY(Particle)=0:NormalZ(Particle)=1 ELSE NormalX(Particle)=NormalX(Particle)/Length:NormalY(Particle)=NormalY(Particle)/Length:NormalZ(Particle)=NormalZ(Particle)/Length
3560   NEXT Particle
3570 SUBEND
3580 ' ----------------------------------------------------------------------
3590 DEF SUB applyAirForces
3600   LOCAL Constraint, A, B, C, EdgeAx, EdgeAy, EdgeAz, EdgeBx, EdgeBy, EdgeBz
3610   LOCAL CrossX, CrossY, CrossZ, CrossLength, FaceNx, FaceNy, FaceNz, MidY, AirX, AirY
3620   LOCAL AirZ, AirNormal, AirSpeed, Pressure, NormalDrag, TangentDrag, FaceForceX, FaceForceY, FaceForceZ
3630   ' Two-sided pressure drag and weak tangential skin drag, per triangle.
3640   FOR Constraint=0 TO TriangleCount-1
3650     A=TriangleA(Constraint) : B=TriangleB(Constraint) : C=TriangleC(Constraint)
3660     EdgeAx=PosX(B)-PosX(A) : EdgeAy=PosY(B)-PosY(A) : EdgeAz=PosZ(B)-PosZ(A)
3670     EdgeBx=PosX(C)-PosX(A) : EdgeBy=PosY(C)-PosY(A) : EdgeBz=PosZ(C)-PosZ(A)
3680     CrossX=EdgeAy*EdgeBz-EdgeAz*EdgeBy : CrossY=EdgeAz*EdgeBx-EdgeAx*EdgeBz : CrossZ=EdgeAx*EdgeBy-EdgeAy*EdgeBx
3690     CrossLength=SQR(CrossX*CrossX+CrossY*CrossY+CrossZ*CrossZ)
3700     IF CrossLength>=0.00000001 THEN
3710       FaceNx=CrossX/CrossLength : FaceNy=CrossY/CrossLength : FaceNz=CrossZ/CrossLength
3720       MidY=(PosY(A)+PosY(B)+PosY(C))/3
3730       AirX=0.35*Wind*SIN(PhysicsTime*0.7+MidY*0.8)-(VelocityX(A)+VelocityX(B)+VelocityX(C))/3
3740       AirY=0.08*Wind*SIN(PhysicsTime*0.5)-(VelocityY(A)+VelocityY(B)+VelocityY(C))/3
3750       AirZ=Wind*(-0.45+0.22*SIN(PhysicsTime*0.8+MidY*0.6))-(VelocityZ(A)+VelocityZ(B)+VelocityZ(C))/3
3760       AirNormal=AirX*FaceNx+AirY*FaceNy+AirZ*FaceNz : AirSpeed=SQR(AirX*AirX+AirY*AirY+AirZ*AirZ)
3770       Pressure=0.5*1.2*(CrossLength*0.5)/3 : NormalDrag=1.1*AirNormal*ABS(AirNormal) : TangentDrag=0.015*AirSpeed
3780       FaceForceX=Pressure*(NormalDrag*FaceNx+TangentDrag*(AirX-AirNormal*FaceNx))
3790       FaceForceY=Pressure*(NormalDrag*FaceNy+TangentDrag*(AirY-AirNormal*FaceNy))
3800       FaceForceZ=Pressure*(NormalDrag*FaceNz+TangentDrag*(AirZ-AirNormal*FaceNz))
3810       ForceX(A)=ForceX(A)+FaceForceX : ForceY(A)=ForceY(A)+FaceForceY : ForceZ(A)=ForceZ(A)+FaceForceZ
3820       ForceX(B)=ForceX(B)+FaceForceX : ForceY(B)=ForceY(B)+FaceForceY : ForceZ(B)=ForceZ(B)+FaceForceZ
3830       ForceX(C)=ForceX(C)+FaceForceX : ForceY(C)=ForceY(C)+FaceForceY : ForceZ(C)=ForceZ(C)+FaceForceZ
3840     ENDIF
3850   NEXT Constraint
3860 SUBEND
3870 ' ----------------------------------------------------------------------
3880 DEF SUB solveSelfContact
3890   LOCAL Particle, Hash, RowA, ColA, OffsetZ, OffsetY, OffsetX, BucketX, BucketY, BucketZ
3900   LOCAL Other, RowB, ColB, WeightA, WeightB, DeltaX, DeltaY, DeltaZ, Length2, Length
3910   LOCAL Correction, BaseBucketX, BaseBucketY, BaseBucketZ, HashYTerm, HashZTerm
3920   ' Repel nearby cloth particles; bucket coordinates stay fixed during a pass.
3930   MAT HashHead=-1
3940   FOR Particle=0 TO ParticleCount-1
3950     HashX(Particle)=INT(PosX(Particle)/SelfDistance) : HashY(Particle)=INT(PosY(Particle)/SelfDistance) : HashZ(Particle)=INT(PosZ(Particle)/SelfDistance)
3960     Hash=(HashX(Particle)*92837111+HashY(Particle)*689287499+HashZ(Particle)*283923481) MOD HashSize
3970     IF Hash<0 THEN Hash=Hash+HashSize
3980     HashNext(Particle)=HashHead(Hash) : HashHead(Hash)=Particle
3990   NEXT Particle
4000   FOR Particle=0 TO ParticleCount-1
4010     RowA=Particle\ParticleStride : ColA=Particle MOD ParticleStride
4011     BaseBucketX=HashX(Particle) : BaseBucketY=HashY(Particle) : BaseBucketZ=HashZ(Particle)
4020     FOR OffsetZ=-1 TO 1
4021       BucketZ=BaseBucketZ+OffsetZ : HashZTerm=BucketZ*283923481
4030       FOR OffsetY=-1 TO 1
4031         BucketY=BaseBucketY+OffsetY : HashYTerm=BucketY*689287499
4040         FOR OffsetX=-1 TO 1
4050           BucketX=BaseBucketX+OffsetX
4060           Hash=(BucketX*92837111+HashYTerm+HashZTerm) MOD HashSize
4070           IF Hash<0 THEN Hash=Hash+HashSize
4080           Other=HashHead(Hash)
4090           WHILE Other>=0
4100             IF Other<=Particle THEN GOTO 4210
4110             IF HashX(Other)<>BucketX OR HashY(Other)<>BucketY OR HashZ(Other)<>BucketZ THEN GOTO 4210
4120             RowB=Other\ParticleStride : ColB=Other MOD ParticleStride
4130             IF ABS(RowA-RowB)<=SelfNeighborSkip AND ABS(ColA-ColB)<=SelfNeighborSkip THEN GOTO 4210
4140             WeightA=InvMass(Particle):WeightB=InvMass(Other):IF WeightA+WeightB=0 THEN GOTO 4210
4150             DeltaX=PosX(Particle)-PosX(Other) : DeltaY=PosY(Particle)-PosY(Other) : DeltaZ=PosZ(Particle)-PosZ(Other)
4160             Length2=DeltaX*DeltaX+DeltaY*DeltaY+DeltaZ*DeltaZ:IF Length2>=SelfDistance*SelfDistance THEN GOTO 4210
4170             Length=SQR(Length2):IF Length<0.00000001 THEN DeltaX=0.00000001:DeltaY=0:DeltaZ=0:Length=0.00000001
4180             Correction=(SelfDistance-Length)/(WeightA+WeightB) : DeltaX=DeltaX/Length : DeltaY=DeltaY/Length : DeltaZ=DeltaZ/Length
4190             PosX(Particle)=PosX(Particle)+WeightA*Correction*DeltaX : PosY(Particle)=PosY(Particle)+WeightA*Correction*DeltaY : PosZ(Particle)=PosZ(Particle)+WeightA*Correction*DeltaZ
4200             PosX(Other)=PosX(Other)-WeightB*Correction*DeltaX : PosY(Other)=PosY(Other)-WeightB*Correction*DeltaY : PosZ(Other)=PosZ(Other)-WeightB*Correction*DeltaZ
4210             Other=HashNext(Other)
4220           WEND
4230         NEXT OffsetX
4240       NEXT OffsetY
4250     NEXT OffsetZ
4260   NEXT Particle
4270 SUBEND
4280 ' ----------------------------------------------------------------------
4290 DEF SUB triangleCross
4300   EdgeAx=PosX(B)-PosX(A) : EdgeAy=PosY(B)-PosY(A) : EdgeAz=PosZ(B)-PosZ(A)
4310   EdgeBx=PosX(C)-PosX(A) : EdgeBy=PosY(C)-PosY(A) : EdgeBz=PosZ(C)-PosZ(A)
4320   CrossX=EdgeAy*EdgeBz-EdgeAz*EdgeBy : CrossY=EdgeAz*EdgeBx-EdgeAx*EdgeBz : CrossZ=EdgeAx*EdgeBy-EdgeAy*EdgeBx
4330   CrossLength=SQR(CrossX*CrossX+CrossY*CrossY+CrossZ*CrossZ) : EXIT SUB
4340 SUBEND
4350 ' ----------------------------------------------------------------------
4360 DEF SUB loadCheckpoint(PosX,PosY,PosZ,VelocityX,VelocityY,VelocityZ)
4370   LOCAL StateVersion, StateCellsX, StateCellsY, StateStepDt, StateBatchHz, StateParticles, StateIters, StateRelax, StateSelf, StateFloor
4380   LOCAL StateThickness, StateWidth, StateDepth, StateSeparation, StateSkip, StateFinal, StateParticle
4390   ' Load physical checkpoint v1-v4; camera and render quality stay configurable.
4400   OPEN StateIn$ FOR INPUT AS #1
4410   INPUT #1,StateVersion,StateCellsX,StateCellsY,StateStepDt,StateBatchHz,FrameIndex,SimTime,PhysicsTime
4420   IF FNcheckpointHeaderMatches(StateVersion,StateCellsX,StateCellsY,StateStepDt,StateBatchHz)=0 THEN CLOSE #1 : PRINT "Saved state configuration mismatch." : SCREEN CLOSE : END
4430   IF StateVersion<4 AND (ClothWidth<>4.6 OR ClothDepth<>3.7) THEN CLOSE #1 : PRINT "Legacy states require the original cloth dimensions." : SCREEN CLOSE : END
4440   IF StateVersion<3 AND CollisionThickness<>.008 THEN CLOSE #1 : PRINT "Legacy states require collision thickness .008." : SCREEN CLOSE : END
4450   IF StateVersion<4 AND (SelfSeparation<>.65 OR SelfNeighborSkip<>2 OR SelfFinalPasses<>0) THEN CLOSE #1 : PRINT "Legacy states require the original self contact settings." : SCREEN CLOSE : END
4460   IF FrameIndex<0 OR FrameIndex<>INT(FrameIndex) OR SimTime<0 OR ABS(SimTime-PhysicsTime)>1E-9 THEN CLOSE #1 : PRINT "Invalid saved state time or frame." : SCREEN CLOSE : END
4470   INPUT #1,Scene,PinMode,Wind,BallX,BallY,BallZ,BallRadius,BallPrevX,BallPrevY,BallPrevZ
4480   INPUT #1,StateParticles
4490   IF StateParticles<>ParticleCount OR Scene<>1 OR PinMode<>0 OR BallRadius<=0 THEN CLOSE #1 : PRINT "Saved state scene mismatch." : SCREEN CLOSE : END
4500   BallStartTime=SimTime : BallStartX=BallX : BallStartY=BallY : BallStartZ=BallZ
4510   IF StateVersion<>1 THEN
4520     IF StateVersion=2 THEN INPUT #1,StateIters,StateRelax,StateSelf,StateFloor,BallMove,BallStartTime,BallStartX,BallStartY,BallStartZ
4530     IF StateVersion=3 THEN INPUT #1,StateIters,StateRelax,StateSelf,StateFloor,BallMove,BallStartTime,BallStartX,BallStartY,BallStartZ,StateThickness
4540     IF StateVersion=4 THEN INPUT #1,StateIters,StateRelax,StateSelf,StateFloor,BallMove,BallStartTime,BallStartX,BallStartY,BallStartZ,StateThickness,StateWidth,StateDepth,StateSeparation,StateSkip,StateFinal
4550     IF StateIters<>XpbdIters OR StateRelax<>RelaxIters OR StateSelf<>SelfContact OR StateFloor<>FloorContact THEN CLOSE #1 : PRINT "Saved state solver mismatch." : SCREEN CLOSE : END
4560     IF StateVersion>=3 AND StateThickness<>CollisionThickness THEN CLOSE #1 : PRINT "Saved state collision thickness mismatch." : SCREEN CLOSE : END
4570     IF StateVersion=4 AND (StateWidth<>ClothWidth OR StateDepth<>ClothDepth) THEN CLOSE #1 : PRINT "Saved state cloth dimensions mismatch." : SCREEN CLOSE : END
4580     IF StateVersion=4 AND (StateSeparation<>SelfSeparation OR StateSkip<>SelfNeighborSkip OR StateFinal<>SelfFinalPasses) THEN CLOSE #1 : PRINT "Saved state self contact mismatch." : SCREEN CLOSE : END
4590   ENDIF
4600   FOR StateParticle=0 TO ParticleCount-1
4610     INPUT #1,PosX(StateParticle),PosY(StateParticle),PosZ(StateParticle),VelocityX(StateParticle),VelocityY(StateParticle),VelocityZ(StateParticle)
4620   NEXT StateParticle
4630   CLOSE #1
4640   MAT OldX=PosX : MAT OldY=PosY : MAT OldZ=PosZ
4650 SUBEND
4660 ' ----------------------------------------------------------------------
4670 DEF SUB saveCheckpoint(PosX,PosY,PosZ,VelocityX,VelocityY,VelocityZ)
4680   LOCAL StateParticle
4690   ' Full checkpoint at a substep boundary: topology, clocks, collider history and velocities.
4700   OPEN StateOut$ FOR OUTPUT AS #1
4710   WRITE #1,4,CellsX,CellsY,StepDt,BatchHz,FrameIndex,SimTime,PhysicsTime
4720   WRITE #1,Scene,PinMode,Wind,BallX,BallY,BallZ,BallRadius,BallPrevX,BallPrevY,BallPrevZ
4730   WRITE #1,ParticleCount
4740   WRITE #1,XpbdIters,RelaxIters,SelfContact,FloorContact,BallMove,BallStartTime,BallStartX,BallStartY,BallStartZ,CollisionThickness,ClothWidth,ClothDepth,SelfSeparation,SelfNeighborSkip,SelfFinalPasses
4750   FOR StateParticle=0 TO ParticleCount-1
4760     WRITE #1,PosX(StateParticle),PosY(StateParticle),PosZ(StateParticle),VelocityX(StateParticle),VelocityY(StateParticle),VelocityZ(StateParticle)
4770   NEXT StateParticle
4780   CLOSE #1
4790 SUBEND
4800 ' ----------------------------------------------------------------------
4810 DEF SUB moveSphere
4820   LOCAL BallElapsed
4830   ' Smooth analytic collider motion evaluated at the same fixed step as the cloth.
4840   IF BallMove=0 THEN EXIT SUB
4850   BallElapsed=SimTime-BallStartTime : BallX=BallStartX+.65*SIN(BallElapsed*.8) : BallY=BallStartY+.2*SIN(BallElapsed*.55) : BallZ=BallStartZ+.5*SIN(BallElapsed*.7)
4860 SUBEND
4870 ' ----------------------------------------------------------------------
4880 DEF SUB advanceBatch
4890   LOCAL Substep
4900   ' Advance STEPS physical substeps for a timing batch.
4910   FOR Substep=1 TO BatchSteps
4920     SimTime=SimTime+StepDt : CALL moveSphere : CALL stepCloth
4930   NEXT Substep
4940   FrameIndex=FrameIndex+1
4950 SUBEND
4960 ' ----------------------------------------------------------------------
4970 DEF SUB advanceStep
4980   ' Cache one physical state, then advance exactly one H-second substep.
4990   CALL cacheDrawState : SimTime=SimTime+StepDt : CALL moveSphere : CALL stepCloth
5000   PhysicsSteps=PhysicsSteps+1 : FrameIndex=FrameIndex+1
5010 SUBEND
5020 ' ----------------------------------------------------------------------
5030 DEF SUB cacheDrawState
5040   ' Render copies are separate from the physical positions and actual sphere.
5050   MAT DrawPrevX=PosX : MAT DrawPrevY=PosY : MAT DrawPrevZ=PosZ
5060   DrawPrevBallX=BallX : DrawPrevBallY=BallY : DrawPrevBallZ=BallZ : DrawPrevTime=SimTime
5070 SUBEND
5080 ' ----------------------------------------------------------------------
5090 DEF SUB catchUp
5100   LOCAL Catchups, LagDrop
5110   ' Bounded catch-up prevents a slow display from accumulating an endless physics backlog.
5120   Catchups=0
5130   WHILE Accumulator>=StepDt AND Catchups<MaxCatchup
5140     CALL advanceStep : Accumulator=MAX(0,Accumulator-StepDt) : Catchups=Catchups+1
5150   WEND
5160   IF Accumulator<StepDt THEN EXIT SUB
5170   LagDrop=INT(Accumulator/StepDt)*StepDt : DroppedTime=DroppedTime+LagDrop : Accumulator=MAX(0,Accumulator-LagDrop)
5180 SUBEND
5190 ' ----------------------------------------------------------------------
5200 DEF SUB initializeMesh
5210   LOCAL GridRow, GridCol, Vertex, FaceV0, FaceV1, FaceV2, FaceV3, BallVertex, SphereX, SphereY
5220   LOCAL SphereZ, SphereRadius, SphereMaterial, SphereBase, SphereRow, SphereTheta, SphereSin, SphereCos, SphereCol, SphereAngle
5230   ' Cloth and one smoothly lit sphere share the perspective depth array.
5240   MaxVertices=ParticleCount+(SphereSegments+1)*(SphereRings+1) : MaxFaces=2*CellsX*CellsY+2*SphereSegments*(SphereRings-1)
5250   DIM VertexX(MaxVertices-1),VertexY(MaxVertices-1),VertexZ(MaxVertices-1),VertexNx(MaxVertices-1),VertexNy(MaxVertices-1),VertexNz(MaxVertices-1),VertexU(MaxVertices-1),VertexV(MaxVertices-1),VertexType(MaxVertices-1)
5260   DIM ScreenX(MaxVertices-1),ScreenY(MaxVertices-1),InvDepth(MaxVertices-1),VertexR(MaxVertices-1),VertexG(MaxVertices-1),VertexB(MaxVertices-1),ShadowU(MaxVertices-1),ShadowV(MaxVertices-1),ShadowD(MaxVertices-1)
5270   DIM DrawPrevX(ParticleCount-1),DrawPrevY(ParticleCount-1),DrawPrevZ(ParticleCount-1)
5280   DIM FaceA(MaxFaces-1),FaceB(MaxFaces-1),FaceC(MaxFaces-1),DepthBuffer(WidthPx-1,HeightPx-1),ShadowBuffer(ShadowSize-1,ShadowSize-1)
5290   DIM DrawFace(MaxFaces-1),SphereVisible(MaxVertices-1)
5300   DIM ClipX(4),ClipY(4),ClipZ(4),ClipR(4),ClipG(4),ClipB(4),ClipIndices(2)
5310   VertexCount=ParticleCount : FaceCount=0 : RenderStride=CellsX+1
5320   FOR GridRow=0 TO CellsY
5330     FOR GridCol=0 TO CellsX
5340       Vertex=GridRow*RenderStride+GridCol : VertexType(Vertex)=0 : VertexU(Vertex)=GridCol/CellsX : VertexV(Vertex)=GridRow/CellsY
5350     NEXT GridCol
5360   NEXT GridRow
5370   FOR GridRow=0 TO CellsY-1
5380     FOR GridCol=0 TO CellsX-1
5390       FaceV0=GridRow*RenderStride+GridCol : FaceV1=FaceV0+1 : FaceV2=FaceV0+RenderStride+1 : FaceV3=FaceV0+RenderStride
5400       FaceA(FaceCount)=FaceV0 : FaceB(FaceCount)=FaceV2 : FaceC(FaceCount)=FaceV1 : FaceCount=FaceCount+1 : FaceA(FaceCount)=FaceV0 : FaceB(FaceCount)=FaceV3 : FaceC(FaceCount)=FaceV2 : FaceCount=FaceCount+1
5410     NEXT GridCol
5420   NEXT GridRow
5430   SphereX=BallX : SphereY=BallY : SphereZ=BallZ : SphereRadius=BallRadius : SphereMaterial=1 : BallFirst=VertexCount : CALL addSphere : BallLast=VertexCount-1
5440   DIM BallOffsetX(BallLast-BallFirst),BallOffsetY(BallLast-BallFirst),BallOffsetZ(BallLast-BallFirst)
5450   FOR Vertex=BallFirst TO BallLast
5460     BallVertex=Vertex-BallFirst : BallOffsetX(BallVertex)=VertexX(Vertex)-BallX : BallOffsetY(BallVertex)=VertexY(Vertex)-BallY : BallOffsetZ(BallVertex)=VertexZ(Vertex)-BallZ
5470   NEXT Vertex
5480 SUBEND
5490 ' ----------------------------------------------------------------------
5500 DEF SUB addSphere
5510   ' Configurable UV sphere, smooth analytic normals, no pole degenerates.
5520   SphereBase=VertexCount
5530   FOR SphereRow=0 TO SphereRings
5540     SphereTheta=PI*SphereRow/SphereRings : SphereSin=SIN(SphereTheta) : SphereCos=COS(SphereTheta)
5550     FOR SphereCol=0 TO SphereSegments
5560       SphereAngle=2*PI*SphereCol/SphereSegments : Vertex=VertexCount : VertexCount=VertexCount+1
5570       VertexNx(Vertex)=SphereSin*COS(SphereAngle) : VertexNy(Vertex)=SphereCos : VertexNz(Vertex)=SphereSin*SIN(SphereAngle)
5580       VertexX(Vertex)=SphereX+SphereRadius*VertexNx(Vertex) : VertexY(Vertex)=SphereY+SphereRadius*VertexNy(Vertex) : VertexZ(Vertex)=SphereZ+SphereRadius*VertexNz(Vertex)
5590       VertexType(Vertex)=SphereMaterial
5600     NEXT SphereCol
5610   NEXT SphereRow
5620   FOR SphereRow=0 TO SphereRings-1
5630     FOR SphereCol=0 TO SphereSegments-1
5640       FaceV0=SphereBase+SphereRow*(SphereSegments+1)+SphereCol : FaceV1=FaceV0+1 : FaceV2=FaceV0+SphereSegments+2 : FaceV3=FaceV0+SphereSegments+1
5650       IF SphereRow>0 THEN FaceA(FaceCount)=FaceV0:FaceB(FaceCount)=FaceV1:FaceC(FaceCount)=FaceV2:FaceCount=FaceCount+1
5660       IF SphereRow<SphereRings-1 THEN FaceA(FaceCount)=FaceV0:FaceB(FaceCount)=FaceV2:FaceC(FaceCount)=FaceV3:FaceCount=FaceCount+1
5670     NEXT SphereCol
5680   NEXT SphereRow
5690 SUBEND
5700 ' ----------------------------------------------------------------------
5710 DEF SUB initializeLights
5720   LOCAL LightLength
5730   ' Warm key, cool fill and a grazing rim. Shadow space is orthographic.
5740   KeyX=-.50 : KeyY=.42 : KeyZ=.76 : LightLength=SQR(KeyX^2+KeyY^2+KeyZ^2) : KeyX=KeyX/LightLength : KeyY=KeyY/LightLength : KeyZ=KeyZ/LightLength
5750   LightUx=KeyZ : LightUy=0 : LightUz=-KeyX : LightLength=SQR(LightUx^2+LightUz^2) : LightUx=LightUx/LightLength : LightUz=LightUz/LightLength
5760   LightVx=KeyY*LightUz : LightVy=KeyZ*LightUx-KeyX*LightUz : LightVz=-KeyY*LightUx : ShadowScale=25*ShadowSize/256 : ShadowLast=ShadowSize-1
5770   FillX=.8 : FillY=.28 : FillZ=.35 : LightLength=SQR(FillX^2+FillY^2+FillZ^2) : FillX=FillX/LightLength : FillY=FillY/LightLength : FillZ=FillZ/LightLength
5780   RimX=.2 : RimY=.45 : RimZ=-.87 : LightLength=SQR(RimX^2+RimY^2+RimZ^2) : RimX=RimX/LightLength : RimY=RimY/LightLength : RimZ=RimZ/LightLength
5790 SUBEND
5800 ' ----------------------------------------------------------------------
5810 DEF SUB drawScene
5820   LOCAL DrawAlpha, Vertex, BallVertex, DeltaX, DeltaY, DeltaZ, CameraDepth, Face, FaceV0, FaceV1
5830   LOCAL FaceV2, ShadowAx, ShadowAy, ShadowAz, ShadowBx, ShadowBy, ShadowBz, ShadowCx, ShadowCy, ShadowCz
5831   LOCAL CastShadow
5840   ' Shader/clip/shadow scratch stays shared by the tiny inner helpers.
5850   ' The normal pass uses shared buffers; checkpoint arrays are passed by reference.
5860   ' Interpolate only render geometry; static BENCHMODE renders the current state exactly.
5870   DrawAlpha=1 : IF BenchMode=0 AND Interpolate THEN DrawAlpha=MAX(0,MIN(1,RenderAlpha))
5880   DrawTime=SimTime : IF DrawAlpha<1 THEN DrawTime=DrawPrevTime+(SimTime-DrawPrevTime)*DrawAlpha
5890   CameraX=Distance*SIN(Azimuth)*COS(Elevation) : CameraY=TargetY+Distance*SIN(Elevation) : CameraZ=Distance*COS(Azimuth)*COS(Elevation)
5900   RightX=COS(Azimuth) : RightZ=-SIN(Azimuth) : UpX=-SIN(Azimuth)*SIN(Elevation) : UpY=COS(Elevation) : UpZ=-COS(Azimuth)*SIN(Elevation)
5910   BackX=SIN(Azimuth)*COS(Elevation) : BackY=SIN(Elevation) : BackZ=COS(Azimuth)*COS(Elevation)
5920   FOR Vertex=0 TO ParticleCount-1
5930     VertexX(Vertex)=PosX(Vertex) : VertexY(Vertex)=PosY(Vertex) : VertexZ(Vertex)=PosZ(Vertex)
5940     IF DrawAlpha<1 THEN VertexX(Vertex)=DrawPrevX(Vertex)+(PosX(Vertex)-DrawPrevX(Vertex))*DrawAlpha:VertexY(Vertex)=DrawPrevY(Vertex)+(PosY(Vertex)-DrawPrevY(Vertex))*DrawAlpha:VertexZ(Vertex)=DrawPrevZ(Vertex)+(PosZ(Vertex)-DrawPrevZ(Vertex))*DrawAlpha
5950   NEXT Vertex
5960   CALL updateDrawNormals
5970   DrawBallX=BallX : DrawBallY=BallY : DrawBallZ=BallZ
5980   IF DrawAlpha<1 THEN DrawBallX=DrawPrevBallX+(BallX-DrawPrevBallX)*DrawAlpha:DrawBallY=DrawPrevBallY+(BallY-DrawPrevBallY)*DrawAlpha:DrawBallZ=DrawPrevBallZ+(BallZ-DrawPrevBallZ)*DrawAlpha
5990   SphereCull=-( (DrawBallX-CameraX)*BackX+(DrawBallY-CameraY)*BackY+(DrawBallZ-CameraZ)*BackZ)>=BallRadius+.15
6000   FOR Vertex=BallFirst TO BallLast
6010     BallVertex=Vertex-BallFirst : VertexX(Vertex)=DrawBallX+BallOffsetX(BallVertex) : VertexY(Vertex)=DrawBallY+BallOffsetY(BallVertex) : VertexZ(Vertex)=DrawBallZ+BallOffsetZ(BallVertex)
6020   NEXT Vertex
6021   IF Shadows THEN
6022     MAT ShadowU=ShadowSize*.5+ShadowScale*(VertexX*LightUx+VertexY*LightUy+VertexZ*LightUz)
6023     MAT ShadowV=ShadowSize*(60/256)+ShadowScale*(VertexX*LightVx+VertexY*LightVy+VertexZ*LightVz)
6024     MAT ShadowD=VertexX*KeyX+VertexY*KeyY+VertexZ*KeyZ
6025   ENDIF
6030   FOR Vertex=0 TO VertexCount-1
6090     DeltaX=VertexX(Vertex)-CameraX : DeltaY=VertexY(Vertex)-CameraY : DeltaZ=VertexZ(Vertex)-CameraZ : CameraDepth=-(DeltaX*BackX+DeltaY*BackY+DeltaZ*BackZ) : InvDepth(Vertex)=1/MAX(.01,CameraDepth)
6100     ScreenX(Vertex)=CentreX+(DeltaX*RightX+DeltaZ*RightZ)*FocalLength*InvDepth(Vertex) : ScreenY(Vertex)=CentreY+(DeltaX*UpX+DeltaY*UpY+DeltaZ*UpZ)*FocalLength*InvDepth(Vertex)
6110   NEXT Vertex
6120   IF Shadows THEN
6130     MAT ShadowBuffer=-1E30
6140     FOR Face=0 TO FaceCount-1
6150       FaceV0=FaceA(Face) : FaceV1=FaceB(Face) : FaceV2=FaceC(Face)
6160       CastShadow=1 : IF VertexType(FaceV0)=1 THEN CastShadow=((ShadowU(FaceV1)-ShadowU(FaceV0))*(ShadowV(FaceV2)-ShadowV(FaceV0))-(ShadowV(FaceV1)-ShadowV(FaceV0))*(ShadowU(FaceV2)-ShadowU(FaceV0))>0)
6161       IF CastShadow THEN
6170         ShadowAx=ShadowU(FaceV0) : ShadowAy=ShadowV(FaceV0) : ShadowAz=ShadowD(FaceV0)
6180         ShadowBx=ShadowU(FaceV1) : ShadowBy=ShadowV(FaceV1) : ShadowBz=ShadowD(FaceV1)
6190         ShadowCx=ShadowU(FaceV2) : ShadowCy=ShadowV(FaceV2) : ShadowCz=ShadowD(FaceV2) : CALL rasterShadow
6200       ENDIF
6210     NEXT Face
6220   ENDIF
6230   ' Only visible sphere faces need lit vertices; the cloth remains two-sided.
6240   MAT DrawFace=1 : MAT SphereVisible=0
6250   IF SphereCull THEN
6260     FOR Face=2*CellsX*CellsY TO FaceCount-1
6270       FaceV0=FaceA(Face) : FaceV1=FaceB(Face) : FaceV2=FaceC(Face)
6280       IF (ScreenX(FaceV1)-ScreenX(FaceV0))*(ScreenY(FaceV2)-ScreenY(FaceV0))-(ScreenY(FaceV1)-ScreenY(FaceV0))*(ScreenX(FaceV2)-ScreenX(FaceV0))<=0 THEN
6290         DrawFace(Face)=0
6300       ELSE
6310         SphereVisible(FaceV0)=1 : SphereVisible(FaceV1)=1 : SphereVisible(FaceV2)=1
6320       ENDIF
6330     NEXT Face
6340   ENDIF
6350   FOR Vertex=0 TO VertexCount-1
6360     IF Vertex<ParticleCount OR SphereCull=0 OR SphereVisible(Vertex) THEN CALL shadeVertex
6370   NEXT Vertex
6380   CLG OFFSCREEN : MAT DepthBuffer=0
6390   FOR Face=0 TO FaceCount-1
6400     IF DrawFace(Face) THEN FaceV0=FaceA(Face) : FaceV1=FaceB(Face) : FaceV2=FaceC(Face) : CALL drawTriangle
6410   NEXT Face
6420 SUBEND
6430 ' ----------------------------------------------------------------------
6440 DEF SUB updateDrawNormals
6450   LOCAL NormalVertex, NormalFace, NormalA, NormalB, NormalC, EdgeAx, EdgeAy, EdgeAz, EdgeBx, EdgeBy
6460   LOCAL EdgeBz, CrossX, CrossY, CrossZ, NormalLength
6470   ' Compute area-weighted normals from the interpolated drawing positions.
6480   FOR NormalVertex=0 TO ParticleCount-1
6490     VertexNx(NormalVertex)=0 : VertexNy(NormalVertex)=0 : VertexNz(NormalVertex)=0
6500   NEXT NormalVertex
6510   FOR NormalFace=0 TO TriangleCount-1
6520     NormalA=TriangleA(NormalFace) : NormalB=TriangleB(NormalFace) : NormalC=TriangleC(NormalFace)
6530     EdgeAx=VertexX(NormalB)-VertexX(NormalA) : EdgeAy=VertexY(NormalB)-VertexY(NormalA) : EdgeAz=VertexZ(NormalB)-VertexZ(NormalA)
6540     EdgeBx=VertexX(NormalC)-VertexX(NormalA) : EdgeBy=VertexY(NormalC)-VertexY(NormalA) : EdgeBz=VertexZ(NormalC)-VertexZ(NormalA)
6550     CrossX=EdgeAy*EdgeBz-EdgeAz*EdgeBy : CrossY=EdgeAz*EdgeBx-EdgeAx*EdgeBz : CrossZ=EdgeAx*EdgeBy-EdgeAy*EdgeBx
6560     VertexNx(NormalA)=VertexNx(NormalA)+CrossX : VertexNy(NormalA)=VertexNy(NormalA)+CrossY : VertexNz(NormalA)=VertexNz(NormalA)+CrossZ
6570     VertexNx(NormalB)=VertexNx(NormalB)+CrossX : VertexNy(NormalB)=VertexNy(NormalB)+CrossY : VertexNz(NormalB)=VertexNz(NormalB)+CrossZ
6580     VertexNx(NormalC)=VertexNx(NormalC)+CrossX : VertexNy(NormalC)=VertexNy(NormalC)+CrossY : VertexNz(NormalC)=VertexNz(NormalC)+CrossZ
6590   NEXT NormalFace
6600   FOR NormalVertex=0 TO ParticleCount-1
6610     NormalLength=SQR(VertexNx(NormalVertex)^2+VertexNy(NormalVertex)^2+VertexNz(NormalVertex)^2)
6620     IF NormalLength<.00000001 THEN VertexNx(NormalVertex)=0:VertexNy(NormalVertex)=0:VertexNz(NormalVertex)=1 ELSE VertexNx(NormalVertex)=VertexNx(NormalVertex)/NormalLength:VertexNy(NormalVertex)=VertexNy(NormalVertex)/NormalLength:VertexNz(NormalVertex)=VertexNz(NormalVertex)/NormalLength
6630   NEXT NormalVertex
6640 SUBEND
6650 ' ----------------------------------------------------------------------
6660 DEF SUB shadeVertex
6670   ' Two-sided satin: anisotropic microfacet highlight, soft sheen and hem.
6680   ShadeNx=VertexNx(Vertex) : ShadeNy=VertexNy(Vertex) : ShadeNz=VertexNz(Vertex)
6690   ViewX=CameraX-VertexX(Vertex) : ViewY=CameraY-VertexY(Vertex) : ViewZ=CameraZ-VertexZ(Vertex)
6700   ViewLength=SQR(ViewX^2+ViewY^2+ViewZ^2)
6710   ViewX=ViewX/ViewLength : ViewY=ViewY/ViewLength : ViewZ=ViewZ/ViewLength
6720   NormalView=ShadeNx*ViewX+ShadeNy*ViewY+ShadeNz*ViewZ : IF NormalView<0 THEN ShadeNx=-ShadeNx:ShadeNy=-ShadeNy:ShadeNz=-ShadeNz:NormalView=-NormalView
6730   NormalKey=MAX(0,ShadeNx*KeyX+ShadeNy*KeyY+ShadeNz*KeyZ) : NormalFill=MAX(0,ShadeNx*FillX+ShadeNy*FillY+ShadeNz*FillZ) : NormalRim=MAX(0,ShadeNx*RimX+ShadeNy*RimY+ShadeNz*RimZ)
6740   ShadeVisibility=1
6750   IF Shadows THEN
6760     ShadowSampleX=ShadowU(Vertex) : ShadowSampleY=ShadowV(Vertex) : ShadowSampleD=ShadowD(Vertex) : ShadowBias=.010+.018*(1-NormalKey) : ShadowGradU=0 : ShadowGradV=0 : ReceiverDot=ShadeNx*KeyX+ShadeNy*KeyY+ShadeNz*KeyZ
6770     IF ABS(ReceiverDot)>.10 THEN ShadowGradU=-(ShadeNx*LightUx+ShadeNy*LightUy+ShadeNz*LightUz)/(ShadowScale*ReceiverDot):ShadowGradV=-(ShadeNx*LightVx+ShadeNy*LightVy+ShadeNz*LightVz)/(ShadowScale*ReceiverDot)
6780     CALL shadowVisibility : ShadeVisibility=.15+.85*Visibility
6790   ENDIF
6800   BaseR=.012 : BaseG=.10 : BaseB=.20 : RoughU=.15 : RoughV=.42
6810   IF Material=1 THEN BaseR=.50:BaseG=.10:BaseB=.035
6820   IF Material=2 THEN BaseR=.70:BaseG=.64:BaseB=.46
6830   IF VertexType(Vertex)=0 THEN Seam=MIN(VertexU(Vertex),1-VertexU(Vertex),VertexV(Vertex),1-VertexV(Vertex)):IF Seam<.018 THEN BaseR=.23:BaseG=.14:BaseB=.055
6840   IF VertexType(Vertex)=1 THEN BaseR=.22:BaseG=.20:BaseB=.18:RoughU=.3:RoughV=.3
6860   IF VertexType(Vertex)=0 THEN
6870     ShadeRow=Vertex\RenderStride : ShadeLeft=MAX(ShadeRow*RenderStride,Vertex-1) : ShadeRight=MIN((ShadeRow+1)*RenderStride-1,Vertex+1)
6880     TangentX=VertexX(ShadeRight)-VertexX(ShadeLeft) : TangentY=VertexY(ShadeRight)-VertexY(ShadeLeft) : TangentZ=VertexZ(ShadeRight)-VertexZ(ShadeLeft)
6890     TangentDot=TangentX*ShadeNx+TangentY*ShadeNy+TangentZ*ShadeNz
6900     TangentX=TangentX-TangentDot*ShadeNx : TangentY=TangentY-TangentDot*ShadeNy : TangentZ=TangentZ-TangentDot*ShadeNz
6910     TangentLength=SQR(TangentX^2+TangentY^2+TangentZ^2)
6911   ELSE
6912     TangentX=1-ShadeNx*ShadeNx : TangentY=-ShadeNx*ShadeNy : TangentZ=-ShadeNx*ShadeNz : TangentLength=SQR(TangentX^2+TangentY^2+TangentZ^2)
6920   ENDIF
6930   IF TangentLength<.01 THEN TangentX=0:TangentY=1-ShadeNy*ShadeNy:TangentZ=-ShadeNy*ShadeNz:TangentLength=SQR(TangentY^2+TangentZ^2)
6940   TangentX=TangentX/MAX(.0001,TangentLength) : TangentY=TangentY/MAX(.0001,TangentLength) : TangentZ=TangentZ/MAX(.0001,TangentLength)
6950   BitangentX=ShadeNy*TangentZ-ShadeNz*TangentY : BitangentY=ShadeNz*TangentX-ShadeNx*TangentZ : BitangentZ=ShadeNx*TangentY-ShadeNy*TangentX
6960   BroadLightX=KeyX : BroadLightY=KeyY : BroadLightZ=KeyZ : CALL computeGgx : KeySpec=Specular*ShadeVisibility
6970   BroadLightX=FillX : BroadLightY=FillY : BroadLightZ=FillZ : CALL computeGgx : FillSpec=Specular
6980   Sheen=(1-NormalView)^4 * (.3+.7*NormalRim) : Diffuse=.08+.78*NormalKey*ShadeVisibility+.16*NormalFill
6990   IF VertexType(Vertex)<>0 THEN Sheen=Sheen*.1
7000   Occlusion=1
7010   IF VertexType(Vertex)=0 THEN Occlusion=.68+.32*MIN(1,MAX(0,(SQR((VertexX(Vertex)-DrawBallX)^2+(VertexY(Vertex)-DrawBallY)^2+(VertexZ(Vertex)-DrawBallZ)^2)-BallRadius)*7))
7020   LinearR=Occlusion*(BaseR*Diffuse+6.75*KeySpec+.9*FillSpec+.08*Sheen) : LinearG=Occlusion*(BaseG*Diffuse+5.6*KeySpec+1.6*FillSpec+.16*Sheen) : LinearB=Occlusion*(BaseB*Diffuse+4.5*KeySpec+2.8*FillSpec+.23*Sheen)
7030   VertexR(Vertex)=255*(LinearR/(1+LinearR))^.4545 : VertexG(Vertex)=255*(LinearG/(1+LinearG))^.4545 : VertexB(Vertex)=255*(LinearB/(1+LinearB))^.4545
7040 SUBEND
7050 ' ----------------------------------------------------------------------
7060 DEF SUB clipTriangle
7070   ' Clip all geometry to a true near plane before perspective rasterization.
7080   ClipIndices(0)=FaceV0 : ClipIndices(1)=FaceV1 : ClipIndices(2)=FaceV2 : ClipCount=0
7090   FOR ClipEdge=0 TO 2
7100     ClipIndexA=ClipIndices(ClipEdge) : ClipIndexB=ClipIndices((ClipEdge+1) MOD 3)
7110     ClipAx=VertexX(ClipIndexA)-CameraX : ClipAy=VertexY(ClipIndexA)-CameraY : ClipAz=VertexZ(ClipIndexA)-CameraZ : ClipAd=-(ClipAx*BackX+ClipAy*BackY+ClipAz*BackZ)
7120     ClipBx=VertexX(ClipIndexB)-CameraX : ClipBy=VertexY(ClipIndexB)-CameraY : ClipBz=VertexZ(ClipIndexB)-CameraZ : ClipBd=-(ClipBx*BackX+ClipBy*BackY+ClipBz*BackZ)
7130     IF ClipAd>=.15 THEN ClipX(ClipCount)=ClipAx:ClipY(ClipCount)=ClipAy:ClipZ(ClipCount)=ClipAz:ClipR(ClipCount)=VertexR(ClipIndexA):ClipG(ClipCount)=VertexG(ClipIndexA):ClipB(ClipCount)=VertexB(ClipIndexA):ClipCount=ClipCount+1
7140     IF (ClipAd>=.15)<>(ClipBd>=.15) THEN
7150       ClipFraction=(.15-ClipAd)/(ClipBd-ClipAd) : ClipX(ClipCount)=ClipAx+(ClipBx-ClipAx)*ClipFraction : ClipY(ClipCount)=ClipAy+(ClipBy-ClipAy)*ClipFraction : ClipZ(ClipCount)=ClipAz+(ClipBz-ClipAz)*ClipFraction
7160       ClipR(ClipCount)=VertexR(ClipIndexA)+(VertexR(ClipIndexB)-VertexR(ClipIndexA))*ClipFraction : ClipG(ClipCount)=VertexG(ClipIndexA)+(VertexG(ClipIndexB)-VertexG(ClipIndexA))*ClipFraction : ClipB(ClipCount)=VertexB(ClipIndexA)+(VertexB(ClipIndexB)-VertexB(ClipIndexA))*ClipFraction : ClipCount=ClipCount+1
7170     ENDIF
7180   NEXT ClipEdge
7190   IF ClipCount<3 THEN EXIT SUB
7200   FOR ClipVertex=0 TO ClipCount-1
7210     ClipDepth=-(ClipX(ClipVertex)*BackX+ClipY(ClipVertex)*BackY+ClipZ(ClipVertex)*BackZ) : ClipInvDepth=1/ClipDepth
7220     ClipScreenX=CentreX+(ClipX(ClipVertex)*RightX+ClipZ(ClipVertex)*RightZ)*FocalLength*ClipInvDepth
7230     ClipScreenY=CentreY+(ClipX(ClipVertex)*UpX+ClipY(ClipVertex)*UpY+ClipZ(ClipVertex)*UpZ)*FocalLength*ClipInvDepth
7240     ClipX(ClipVertex)=ClipScreenX : ClipY(ClipVertex)=ClipScreenY : ClipZ(ClipVertex)=ClipInvDepth
7250   NEXT ClipVertex
7260   FOR ClipVertex=1 TO ClipCount-2
7270     RasterAx=ClipX(0) : RasterAy=ClipY(0) : RasterAq=ClipZ(0) : RasterAr=ClipR(0) : RasterAg=ClipG(0) : RasterAb=ClipB(0)
7280     RasterBx=ClipX(ClipVertex) : RasterBy=ClipY(ClipVertex) : RasterBq=ClipZ(ClipVertex) : RasterBr=ClipR(ClipVertex) : RasterBg=ClipG(ClipVertex) : RasterBb=ClipB(ClipVertex)
7290     RasterCx=ClipX(ClipVertex+1) : RasterCy=ClipY(ClipVertex+1) : RasterCq=ClipZ(ClipVertex+1) : RasterCr=ClipR(ClipVertex+1) : RasterCg=ClipG(ClipVertex+1) : RasterCb=ClipB(ClipVertex+1) : GTRIANGLE DepthBuffer,RasterAx,RasterAy,RasterAq,RasterAr,RasterAg,RasterAb,RasterBx,RasterBy,RasterBq,RasterBr,RasterBg,RasterBb,RasterCx,RasterCy,RasterCq,RasterCr,RasterCg,RasterCb
7300   NEXT ClipVertex
7310 SUBEND
7320 ' ----------------------------------------------------------------------
7330 DEF SUB drawTriangle
7340   ' Reuse projected vertices when the whole triangle is in front of the near plane.
7350   IF InvDepth(FaceV0)>1/.15 OR InvDepth(FaceV1)>1/.15 OR InvDepth(FaceV2)>1/.15 THEN CALL clipTriangle : EXIT SUB
7360   RasterAx=ScreenX(FaceV0) : RasterAy=ScreenY(FaceV0) : RasterAq=InvDepth(FaceV0) : RasterAr=VertexR(FaceV0) : RasterAg=VertexG(FaceV0) : RasterAb=VertexB(FaceV0)
7370   RasterBx=ScreenX(FaceV1) : RasterBy=ScreenY(FaceV1) : RasterBq=InvDepth(FaceV1) : RasterBr=VertexR(FaceV1) : RasterBg=VertexG(FaceV1) : RasterBb=VertexB(FaceV1)
7380   RasterCx=ScreenX(FaceV2) : RasterCy=ScreenY(FaceV2) : RasterCq=InvDepth(FaceV2) : RasterCr=VertexR(FaceV2) : RasterCg=VertexG(FaceV2) : RasterCb=VertexB(FaceV2)
7390   GTRIANGLE DepthBuffer,RasterAx,RasterAy,RasterAq,RasterAr,RasterAg,RasterAb,RasterBx,RasterBy,RasterBq,RasterBr,RasterBg,RasterBb,RasterCx,RasterCy,RasterCq,RasterCr,RasterCg,RasterCb
7400 SUBEND
7410 ' ----------------------------------------------------------------------
7420 DEF SUB rasterShadow
7430   ' Raster a two-sided triangle into the configured shadow map; larger depth faces light.
7440   ' ShadowAx/Ay/Az, ShadowBx/By/Bz and ShadowCx/Cy/Cz stay unchanged.
7441   ' The 128 map with 3x3 taps keeps the original 256 / 5x5 filter footprint.
7450   ' Integer texel centres; triangles may extend beyond all four map edges.
7460   ' The caller dimensions ShadowBuffer and clears it to -1E30 before the shadow pass.
7470   ShadowDet=(ShadowBx-ShadowAx)*(ShadowCy-ShadowAy)-(ShadowBy-ShadowAy)*(ShadowCx-ShadowAx)
7480   IF ABS(ShadowDet)<1E-10 THEN EXIT SUB
7490   ShadowMinX=MAX(0,-INT(-MIN(ShadowAx,ShadowBx,ShadowCx))) : ShadowMaxX=MIN(ShadowLast,INT(MAX(ShadowAx,ShadowBx,ShadowCx)))
7500   ShadowMinY=MAX(0,-INT(-MIN(ShadowAy,ShadowBy,ShadowCy))) : ShadowMaxY=MIN(ShadowLast,INT(MAX(ShadowAy,ShadowBy,ShadowCy)))
7510   IF ShadowMinX>ShadowMaxX OR ShadowMinY>ShadowMaxY THEN EXIT SUB
7520   ShadowInv=1/ShadowDet
7530   ShadowStepAx=(ShadowBy-ShadowCy)*ShadowInv : ShadowStepAy=(ShadowCx-ShadowBx)*ShadowInv
7540   ShadowStepBx=(ShadowCy-ShadowAy)*ShadowInv : ShadowStepBy=(ShadowAx-ShadowCx)*ShadowInv
7550   ShadowRowA=((ShadowBx-ShadowMinX)*(ShadowCy-ShadowMinY)-(ShadowBy-ShadowMinY)*(ShadowCx-ShadowMinX))*ShadowInv
7560   ShadowRowB=((ShadowCx-ShadowMinX)*(ShadowAy-ShadowMinY)-(ShadowCy-ShadowMinY)*(ShadowAx-ShadowMinX))*ShadowInv
7570   ShadowStepDepthX=ShadowStepAx*(ShadowAz-ShadowCz)+ShadowStepBx*(ShadowBz-ShadowCz)
7580   ShadowStepDepthY=ShadowStepAy*(ShadowAz-ShadowCz)+ShadowStepBy*(ShadowBz-ShadowCz)
7590   ShadowRowDepth=ShadowCz+ShadowRowA*(ShadowAz-ShadowCz)+ShadowRowB*(ShadowBz-ShadowCz)
7600   FOR ShadowY=ShadowMinY TO ShadowMaxY
7610     ShadowWeightA=ShadowRowA : ShadowWeightB=ShadowRowB : ShadowDepth=ShadowRowDepth
7620     FOR ShadowX=ShadowMinX TO ShadowMaxX
7630       IF ShadowWeightA>=-1E-9 AND ShadowWeightB>=-1E-9 AND ShadowWeightA+ShadowWeightB<=1.000000001 THEN IF ShadowDepth>ShadowBuffer(ShadowX,ShadowY) THEN ShadowBuffer(ShadowX,ShadowY)=ShadowDepth
7660       ShadowWeightA=ShadowWeightA+ShadowStepAx : ShadowWeightB=ShadowWeightB+ShadowStepBx : ShadowDepth=ShadowDepth+ShadowStepDepthX
7670     NEXT ShadowX
7680     ShadowRowA=ShadowRowA+ShadowStepAy : ShadowRowB=ShadowRowB+ShadowStepBy : ShadowRowDepth=ShadowRowDepth+ShadowStepDepthY
7690   NEXT ShadowY
7700 SUBEND
7710 ' ----------------------------------------------------------------------
7720 DEF SUB shadowVisibility
7730   ' Filtered shadow query: ShadowSampleX/Y/D -> Visibility in [0,1]; inputs stay unchanged.
7740   ' ShadowPcf=5 selects 5x5; other values use 3x3. All taps have equal weight.
7750   ' ShadowBias>0 overrides the default 0.025 metre comparison bias.
7760   ' Samples outside 0..ShadowLast are lit; coordinates follow ShadowScale.
7770   ' ShadowGradU and ShadowGradV are receiver depth gradients (metres/texel), read only.
7780   Visibility=1 : IF ShadowSampleX<0 OR ShadowSampleX>ShadowLast OR ShadowSampleY<0 OR ShadowSampleY>ShadowLast THEN EXIT SUB
7790   ShadowCentreX=INT(ShadowSampleX+0.5) : ShadowCentreY=INT(ShadowSampleY+0.5)
7800   ShadowRadius=1 : IF ShadowPcf=5 THEN ShadowRadius=2
7810   ShadowCut=ShadowSampleD+0.025 : IF ShadowBias>0 THEN ShadowCut=ShadowSampleD+ShadowBias
7820   ShadowLit=0
7830   FOR ShadowTapV=-ShadowRadius TO ShadowRadius
7840     ShadowTapY=ShadowCentreY+ShadowTapV : ShadowRowCut=ShadowCut+ShadowGradV*(ShadowTapY-ShadowSampleY)
7850     FOR ShadowTapU=-ShadowRadius TO ShadowRadius
7860       ShadowTapX=ShadowCentreX+ShadowTapU : ShadowTapCut=ShadowRowCut+ShadowGradU*(ShadowTapX-ShadowSampleX)
7870       IF ShadowTapX<0 OR ShadowTapX>ShadowLast OR ShadowTapY<0 OR ShadowTapY>ShadowLast THEN ShadowLit=ShadowLit+1 ELSE IF ShadowTapCut>=ShadowBuffer(ShadowTapX,ShadowTapY) THEN ShadowLit=ShadowLit+1
7880     NEXT ShadowTapU
7890   NEXT ShadowTapV
7900   Visibility=ShadowLit/(2*ShadowRadius+1)^2
7910 SUBEND
7920 ' ----------------------------------------------------------------------
7930 DEF SUB computeGgx
7940   ' Anisotropic GGX for one broad studio light; tangent follows the material warp.
7950   HalfX=ViewX+BroadLightX : HalfY=ViewY+BroadLightY : HalfZ=ViewZ+BroadLightZ : HalfLength=SQR(HalfX^2+HalfY^2+HalfZ^2) : HalfX=HalfX/HalfLength : HalfY=HalfY/HalfLength : HalfZ=HalfZ/HalfLength
7960   NormalHalf=MAX(.001,ShadeNx*HalfX+ShadeNy*HalfY+ShadeNz*HalfZ) : TangentHalf=HalfX*TangentX+HalfY*TangentY+HalfZ*TangentZ : BitangentHalf=HalfX*BitangentX+HalfY*BitangentY+HalfZ*BitangentZ
7970   NormalLight=MAX(0,ShadeNx*BroadLightX+ShadeNy*BroadLightY+ShadeNz*BroadLightZ)
7980   Distribution=1/(PI*RoughU*RoughV*((TangentHalf/RoughU)^2+(BitangentHalf/RoughV)^2+NormalHalf^2)^2) : ViewHalf=MAX(.0001,HalfX*ViewX+HalfY*ViewY+HalfZ*ViewZ) : Fresnel=.045+.955*(1-ViewHalf)^5
7990   Geometry=MIN(1,2*NormalHalf*MIN(NormalView,NormalLight)/ViewHalf) : Specular=MIN(2,Distribution*Fresnel*Geometry/(4*MAX(.06,NormalView)))
8000 SUBEND
8010 ' ----------------------------------------------------------------------
8020 DEF SUB drawHud
8030   LOCAL ShadowText$, PauseText$
8040   ' Display controls, mesh size, simulation rate and measured FPS.
8050   ShadowText$="OFF" : IF Shadows THEN ShadowText$="ON"
8060   PauseText$="" : IF Paused THEN PauseText$=" [PAUSED]"
8070   INK RGB(215,220,232) : LOCATE 3,2
8080   GPRINT "FPS: ";INT(Fps+.5);"  PHYS: ";INT(1/StepDt+.5);"HZ  SPACE: PAUSE  N: STEP  S: SHADOWS ";ShadowText$;"  R: RESTART";PauseText$
8090 SUBEND
8100 ' ----------------------------------------------------------------------
8110 DEF SUB dragCloth
8120   LOCAL MouseHeld, MouseXpx, MouseYpx, GrabVertex, GrabDistance2, GrabScreenX, GrabScreenY, GrabPlaneX, GrabPlaneY, GrabTargetX
8130   LOCAL GrabTargetY, GrabTargetZ, GrabDx, GrabDy, GrabDz, GrabLength, GrabScale
8140   ' Pick a visible node and drag it in the camera plane at its original hit depth.
8150   MouseHeld=ABS(MOUSELEFT) : MouseXpx=MOUSEX : MouseYpx=MOUSEY
8160   IF MouseHeld=0 THEN Grab=-1:LastLeft=0:EXIT SUB
8170   IF LastLeft=0 THEN
8180     LastLeft=1 : GrabBest=28^2 : Grab=-1
8190     IF MouseXpx<0 OR MouseXpx>=WidthPx OR MouseYpx<0 OR MouseYpx>=HeightPx THEN EXIT SUB
8200     FOR GrabVertex=0 TO ParticleCount-1
8210       GrabDistance2=(ScreenX(GrabVertex)-MouseXpx)^2+(ScreenY(GrabVertex)-MouseYpx)^2
8220       IF GrabDistance2<GrabBest THEN
8230         GrabScreenX=INT(ScreenX(GrabVertex)) : GrabScreenY=INT(ScreenY(GrabVertex))
8240         IF GrabScreenX>=0 AND GrabScreenX<WidthPx AND GrabScreenY>=0 AND GrabScreenY<HeightPx THEN
8250           IF ABS(InvDepth(GrabVertex)-DepthBuffer(GrabScreenX,GrabScreenY))<.05*InvDepth(GrabVertex)^2+.0005 THEN Grab=GrabVertex:GrabBest=GrabDistance2
8260         ENDIF
8270       ENDIF
8280     NEXT GrabVertex
8290     IF Grab>=0 THEN GrabDepth=1/InvDepth(Grab)
8300   ENDIF
8310   IF Grab<0 THEN EXIT SUB
8320   GrabPlaneX=(MouseXpx-CentreX)*GrabDepth/FocalLength : GrabPlaneY=(MouseYpx-CentreY)*GrabDepth/FocalLength
8330   GrabTargetX=CameraX-BackX*GrabDepth+RightX*GrabPlaneX+UpX*GrabPlaneY : GrabTargetY=CameraY-BackY*GrabDepth+UpY*GrabPlaneY : GrabTargetZ=CameraZ-BackZ*GrabDepth+RightZ*GrabPlaneX+UpZ*GrabPlaneY
8340   GrabDx=GrabTargetX-PosX(Grab) : GrabDy=GrabTargetY-PosY(Grab) : GrabDz=GrabTargetZ-PosZ(Grab)
8350   GrabLength=SQR(GrabDx^2+GrabDy^2+GrabDz^2) : GrabScale=MIN(1,.12/MAX(.0001,GrabLength))
8360   PosX(Grab)=PosX(Grab)+GrabScale*GrabDx : PosY(Grab)=PosY(Grab)+GrabScale*GrabDy : PosZ(Grab)=PosZ(Grab)+GrabScale*GrabDz
8370   GrabDx=PosX(Grab)-BallX : GrabDy=PosY(Grab)-BallY : GrabDz=PosZ(Grab)-BallZ : GrabLength=SQR(GrabDx^2+GrabDy^2+GrabDz^2)
8380   IF GrabLength<.00000001 THEN GrabDx=0:GrabDy=0:GrabDz=.00000001:GrabLength=.00000001
8390   IF GrabLength<BallRadius+ContactThickness THEN GrabScale=(BallRadius+ContactThickness)/MAX(.0001,GrabLength):PosX(Grab)=BallX+GrabDx*GrabScale:PosY(Grab)=BallY+GrabDy*GrabScale:PosZ(Grab)=BallZ+GrabDz*GrabScale
8400   IF FloorContact THEN PosY(Grab)=MAX(.035+ContactThickness,PosY(Grab))
8410   OldX(Grab)=PosX(Grab) : OldY(Grab)=PosY(Grab) : OldZ(Grab)=PosZ(Grab)
8420 SUBEND
8430 ' ----------------------------------------------------------------------
8440 DEF SUB readControls
8450   LOCAL Command$
8460   ' Read keyboard controls and handle mouse dragging.
8470   Command$=UPPER$(INKEY$) : IF Command$=CHR$(27) THEN Done=1:EXIT SUB
8480   IF Command$=" " THEN Paused=1-Paused:Accumulator=0:WallDt=0:LastWall=TIME:RenderAlpha=1:CALL cacheDrawState
8490   IF Command$="N" AND Paused THEN SingleStep=1
8500   IF Command$="S" THEN Shadows=1-Shadows
8510   IF Command$="M" THEN Material=(Material+1) MOD 3
8520   IF Command$="W" THEN Wind=1-Wind
8530   IF Command$="B" THEN BallMove=1-BallMove:BallStartTime=SimTime:BallStartX=BallX:BallStartY=BallY:BallStartZ=BallZ
8540   IF Command$="+" OR Command$="=" THEN Distance=MAX(4.8,Distance-.3)
8550   IF Command$="-" THEN Distance=MIN(12,Distance+.3)
8560   IF Command$="R" THEN CALL restartCloth : EXIT SUB
8570   IF KEYDOWN(28) THEN Azimuth=Azimuth-.8*InputDt
8580   IF KEYDOWN(29) THEN Azimuth=Azimuth+.8*InputDt
8590   IF KEYDOWN(30) THEN Elevation=MIN(.8,Elevation+.6*InputDt)
8600   IF KEYDOWN(31) THEN Elevation=MAX(-.15,Elevation-.6*InputDt)
8610   IF Paused THEN Grab=-1:LastLeft=0:EXIT SUB
8620   CALL dragCloth
8630 SUBEND
8640 ' ----------------------------------------------------------------------
8650 DEF SUB restartCloth
8660   ' Restart the fall from a flat sheet.
8670   SimTime=0 : FrameIndex=0 : Paused=0 : SingleStep=0
8680   BallX=0 : BallY=1.45 : BallZ=-.25 : BallStartTime=0 : BallStartX=BallX : BallStartY=BallY : BallStartZ=BallZ
8690   Grab=-1 : LastLeft=0 : Accumulator=0 : WallDt=0
8700   CALL initializeCloth : CALL cacheDrawState : RenderAlpha=1
8710   LastWall=TIME : FpsCount=0 : FpsLast=LastWall : Fps=0
8720 SUBEND
8730 ' Match a saved header to the current fixed-step grid before reading its arrays.
8740 DEF FNcheckpointHeaderMatches(SavedVersion,SavedCellsX,SavedCellsY,SavedStepDt,SavedBatchHz)
8750   IF SavedVersion<>1 AND SavedVersion<>2 AND SavedVersion<>3 AND SavedVersion<>4 THEN EXIT FN
8760   IF SavedCellsX<>CellsX OR SavedCellsY<>CellsY OR SavedStepDt<>StepDt OR SavedBatchHz<>BatchHz THEN EXIT FN
8770   FNcheckpointHeaderMatches=1
8780 FNEND
