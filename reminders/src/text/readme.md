# Reminder command

There are three different kinds of reminders see [this struct](../interface.rs#262):
- "once", one-time reminders (like "remind me in 20 mins ...")
- "recurrent", ever repeating reminders (like "remind me every 12th of January")
- "recurrent until", reminders that repeat until a set moment (like "remind me every thursday from April to November")

## Inference mechanism

Depending on the keywords, Ambrog.io will do its best to infer the type of the reminder. 
The inference mechanism is implemented at [this file](parsing.rs).

Overall, the code consumes tokens from left to right everytime it recognises words depending on the context.
Roughly, context is modelled with function scopes.

## Overall command syntax

The language is Italian, timezone is `Europe/Rome`, and the syntax is as follows:
> _Ricordami \<TIME_EXPR>:_
> 
> _\<MULTILINE_MESSAGE>_

- `TIME_EXPR` is the phrase used to instruct the reminder
- `MULTILINE_MESSAGE` is the message (can contain more lines) to send at each repetition of the reminder

## Time expressions

As mentioned before, we have three kinds: once, recurrent, and recurrent until.

**⚠️ WARNING:** you can try mixing all pieces, but the result is not well-defined. The overall inference is best-effort and does not care about unexpected uses of the "language constructs". In other words: the harder is the reminder expression, the less predictable is the result.

### Once expressions

You can do like the following examples (see [these tests](parsing.rs#734)):
- `ricordami giovedì`: reminds the next available Thursday, at the same time your command has been received (if you issue it at 2PM, the reminder will be set at 2PM of next)
- `ricordami tra 5 minuti`: reminds in the next 5 minutes. You can do this for `secondi`, `minuti`, `ore`, `giorni`, `settimane`
    - you can also concatenate multiple `tra ...` with `e`: `ricordami tra 1 minuto e 20 secondi`
- `ricordami nel 2025`: reminds on the same date, but in the 2025
- `ricordami il 12`: reminds on the next 12th day of the month
- `ricordami ad agosto`: if you're after August, it'll be set to August of next year. Same day and same time though.
- `ricordami alle 10`: if it's already past 10AM, go to the 10AM next day
- `ricordami il 20 dicembre alle 9:30`: next available 20th Dec at 9:30AM
- `ricordami sabato alle 15`: next available Saturday at 3PM

### Recurrent expressions

By default:
- ([ref](parsing.rs#118)) the reminder have time set to the command reception time (if you issue the command at 2PM, the reminder will have 2PM as time)
- ([ref](parsing.rs#138)) the reminder will always have starting year as the `since`
- ([ref](parsing.rs#115)) `since` is set to the command reception time
- recurrence is at "every day"

You can do like the following examples (see [these tests](parsing.rs#896)):
- `ricordami ogni 2 anni`: reminds every 2 years starting from now
- `ricordami ogni 2 anni ogni 7 maggio`: reminds every 2 years from now on every 7th May
- `ricordami ogni sabato alle 13`: reminds every Saturday from now at 1PM
- `ricordami ogni secondo e terzo lunedì e mercoledì alle 13, 14, 15`: reminds every second and third monday, every second and third wednesday, at 1PM, 2PM, and 3PM
- `ricordami ogni 4, 10, 20 di luglio, agosto, e dicembre alle 20`: reminds every 4th, 10th, 20th of July, August, and December at 8PM
- `ricordami ogni sabato da giugno 2025 ad aprile 2026`: reminds on each Saturday since June 2025 and until April 2026 (uses day and time from command issuing)
- `ricordami fino al 2030 ogni 2 anni ogni 01/10 alle 13`: reminds every 1st October at 1PM each 2 years, starting from now and until 2030
- `ricordami dal 13 novembre al 20 dicembre ogni venerdì alle 14`: reminds every Friday from 13th Nov until 20th Dec at 2PM

**⚠️ WARNING:** when you specify `alle` right after a `until` or `since` definition, the time setting is referred to that boundary, not the reminder scheduling.