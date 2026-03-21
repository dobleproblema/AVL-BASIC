100 DEF FNSOL$=CHR$(INT(RND*26+97))
110 s$=FNSOL$
120 EVERY 10,1 GOSUB 250  'Generate one guess every 0.2 seconds
130 EVERY 5,2 GOSUB 290   'Check the current guess every 0.1 seconds
140 AFTER 250,0 GOSUB 220 'End after 5 seconds
150 PRINT "Will I guess the letter I picked myself (";UPPER$(s$);") in 5 seconds? ;)"
160 PAUSE
170 END
180 REM The latest guess was correct
190 PRINT : PRINT "'";UPPER$(a$);"' is correct. I win!"
200 BEEP : END
210 REM Time is up
220 PRINT : PRINT "Too late. I still win! :DDD"
230 END
240 REM Generate and print one guess
250 a$=FNSOL$
260 PRINT UPPER$(a$);" ";
270 RETURN
280 REM Check whether the current guess is correct
290 IF a$=s$ THEN DI:GOTO 190 'If so, disable interrupts before continuing
300 RETURN

