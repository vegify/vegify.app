# Site Map

Reconstructed from the `Site Map` page of `Vegify.sketch` (the page titles and numbering lived in
symbol-instance overrides, which the Figma import dropped). Numbering is preserved verbatim.

**Solid edges** = hierarchy encoded by the numbering scheme. **Dashed edges** = flow arrows that
were explicitly drawn and named in the sketch (`welcome --> register/login`,
`register/login --> home/"newsfeed"`, `how to plan --> calculate requirements`). The sketch's other
connector graphics were duplicated symbols with stale names and were not treated as data.

```mermaid
flowchart TD
  subgraph s1["1.0.0 · Home Page"]
    n110["1.1.0 Welcome Screen"]
    n120["1.2.0 How to Meal Plan"]
    n130["1.3.0 Login / Sign Up"]
    n131["1.3.1 Calculating Requirements"]
    n132["1.3.2 Success Confirmation"]
    n140["1.4.0 Home / “Newsfeed”"]
    n150["1.5.0 Settings"]
    n151["1.5.1 About"]
    n130 --> n131
    n130 --> n132
    n150 --> n151
    n110 -.-> n130
    n130 -.-> n140
    n120 -.-> n131
  end

  subgraph s2["2.0.0 · Search"]
    n210["2.1.0 Search Page"]
    n211["2.1.1 Search Results"]
    n210 --> n211
  end

  subgraph s3["3.0.0 · Recipes"]
    n310["3.1.0 View Recipe"]
    n311["3.1.1 Nutrition Info"]
    n320["3.2.0 Create/Edit Recipe"]
    n321["3.2.1 Add Ingredient…"]
    n3211["3.2.1.1 Search Ingredients"]
    n3212["3.2.1.2 Ingredient Search Results"]
    n322["3.2.2 Scan Label"]
    n323["3.2.3 Create / Edit Ingredient"]
    n330["3.3.0 View Ingredient"]
    n310 --> n311
    n320 --> n321
    n321 --> n3211
    n321 --> n3212
    n320 --> n322
    n320 --> n323
  end

  subgraph s4["4.0.0 · Meal Planning"]
    n410["4.1.0 View/Edit Meal Plan"]
    n411["4.1.1 Nutrition Info"]
    n420["4.2.0 Meal Plan Calendar"]
    n421["4.2.1 Shopping List"]
    n410 --> n411
    n420 --> n421
  end

  subgraph s5["5.0.0 · Social"]
    n510["5.1.0 Profile"]
    n511["5.1.1 My Recipes"]
    n512["5.1.2 My Meal Plans"]
    n513["5.1.3 People I’m Following"]
    n514["5.1.4 People Following Me"]
    n520["5.2.0 Notifications"]
    n530["5.3.0 Inbox"]
    n531["5.3.1 Message"]
    n540["5.4.0 Share"]
    n510 --> n511
    n510 --> n512
    n510 --> n513
    n510 --> n514
    n530 --> n531
  end
```
