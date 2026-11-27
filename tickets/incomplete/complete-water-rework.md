We have implemented custom water to varying degrees of success. our custom implementation is buggy and lacking many features. I would like to completely do away with our water system in favor of slotting in a completely new system. This system can be found at https://github.com/wave-harmonic/crest. It will be your job to port this with as much integrity as possible. 

for this you will do the follwoing.
1. Create a harness for testing the accurracy of your port without running the main game
   1. this will require running and testing both systems and matching goldens amongst other things
2. port over the crest water system in its entirety
3. ensure ant stragglers from our old water implementation are cleaned up

