# Text overrides in Vegify.sketch

Every symbol-instance **text override** in the file whose value differs from the symbol
master's default text. Figma's .sketch import drops instance overrides, so in Figma these
instances silently show the *default* text — the real content is the **override** value here.

Extracted from `Vegify.sketch`: 433 instances carry overrides (399 text overrides, of which 392 differ from the default; 318 are listed below, the other 74 are the Site Map entries, promoted into a diagram at [../site-map.md](../site-map.md); plus 6 image, 146 symbol-swap, and 113 style overrides).
Full machine-readable dump incl. Site Map and no-op overrides: [text-overrides.json](text-overrides.json).

## Page/3.Recipes/2.Add/1.0.Add Ingredient

### Page/Recipes/Add/Search for Ingredient/Desktop HD

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create / Edit Recipe"**

### Page/Recipes/Add/Search for Ingredient/Mobile

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create / Edit Recipe"**

## Page/3.Recipes/2.Add/3.Ingredient/1.Search

### Page/Recipes/Add/Search Results/Desktop HD

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"Recipe"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Add Ingredient"**

### Page/Recipes/Add/Search Results/Mobile

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"Recipe"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Add Ingredient"**

## User Personas

### Artboard

- **Marcus** — master `UX / Persona Template`
  - `Name` (default: "Name") → **"Marcus"**
  - `Occupation` (default: "Occupation") → **"Librarian"**
  - `Full Name` (default: "Full Name") → **"Marcus Reed"**
  - `Age` (default: "Age") → **"37"**
  - `Eating Style` (default: "Eating Style") → **"Omnivore"**
  - `Big Text` (default: "  	    Who is the user? \n_.    What is the user’s history w…"):
    >      Marcus has eaten some vegan       _   meals, mainly through meal-delivery service Purple Carrot. He isn’t strictly vegan, but he does largely enjoy the food. 
    >
    > He has never personally veganized a recipe before. 
    >
    > He has tracked some macronutrients before with LoseIt, but never micronutrients. He and his wife did meal-planning with Platejoy and rather liked the UI, although he found the recipes to be limited & bland, or often containing difficult-to-find ingredients.
    >
    > Marcus knows of Vegify through its social media presence. He and his wife have discussed some of the recipes, but haven’t yet prepared any of them.
    >
    >
    >
    >
  - `Consults recipes on...` (default: "Consults recipes on…") → **"Phone"**
  - `Prefers to plan meals...` (default: "Prefers to plan meals…") → **"1 day at a time; no more than 3"**
  - `Pain Points` (default: "	• 	Pain\n	• 	Points\n	• 	For\n	• 	The\n	• 	End\n	• 	User"):
    > • Adhering to diet
    > • Limited/bland recipes on other apps
    > • Difficult-to-find ingredients in other recipe apps
    > •
  - `Consults outside source?` (default: "Consults outside source?") → **"No. They tend to give the same advice, and he's struggled to comply in the past, so no longer asks."**

- **Priya** — master `UX / Persona Template`
  - `Name` (default: "Name") → **"Priya"**
  - `Occupation` (default: "Occupation") → **"Microbiologist"**
  - `Full Name` (default: "Full Name") → **"Priya Desai"**
  - `Age` (default: "Age") → **"30"**
  - `Eating Style` (default: "Eating Style") → **"Vegan When Possible"**
  - `Big Text` (default: "  	    Who is the user? \n_.    What is the user’s history w…"):
    >   	    Priya chooses vegan food        _  where possible, but will accept non-vegan food when it’s the only option. She hasn’t eaten meat in about two years, and was never a big fan of dairy. 
    >
    > She has never veganized a recipe.
    >
    > Priya has used completefoods.co to read recipes, but never really updated or tracked them. She found the UI to be subpar in its flow for inputting ingredients & recipes.
    >
    > Priya has been in two of Vegify’s YouTube videos: the cheese tasteoff & the ice cream tasteoff. She was instrumental in the discovery of the design flaw of other apps, having been an early recipient of shared recipes and meal plans. She thinks Vegify will be very helpful for people.
    >
    > After viewing a Vegify video, Priya frequently gives a thumbs up and leaves a comment.
  - `Prefers to plan meals...` (default: "Prefers to plan meals…") → **"Multiple days in advance"**
  - `Consults recipes on...` (default: "Consults recipes on…") → **"Phone"**
  - `Consults outside source?` (default: "Consults outside source?"):
    > Yes. Consults with her rheumetologist due to her auto-immune condition.  Has previously consulted an RDN.
  - `Pain Points` (default: "	• 	Pain\n	• 	Points\n	• 	For\n	• 	The\n	• 	End\n	• 	User"):
    > • UI for ingredient input on competing apps
    > • UX of recipe updating & sharing for same

- **Wesley** — master `UX / Persona Template`
  - `Name` (default: "Name") → **"Wesley"**
  - `Occupation` (default: "Occupation") → **"Product Manager"**
  - `Full Name` (default: "Full Name") → **"Wesley Hartley"**
  - `Eating Style` (default: "Eating Style") → **"Vegan"**
  - `Age` (default: "Age") → **"33"**
  - `Big Text` (default: "  	    Who is the user? \n_.    What is the user’s history w…"):
    >   	   Wesley went pescatarian in July      _    2013, then vegan July 2016, and has been vegan ever since.
    >
    > Wesley actively seeks out methods for veganizing recipes, citing examples such as fish tacos, burgers, and (his personal favorite) hot wings as success stories. When veganizing, he usually starts by replacing dairy cheeses with their direct vegan substitues, and from there attempts to find creative solutions for products without clear substitutions. He doesn’t typically use meat substitutes on pizza (or bacon substitutes at all), but doesn’t seem opposed to meat substitutes in general.
    >
    > Wesley has not tracked his dietary intake until very recently. when he was diagnosed with a vitamin D deficiency. He has never used an app for tracking meals.
    >
    > Wesley has seen early sketches for Vegify and felt it looked promising. After viewing Vegify content, he tends to introspect on his nutrition.
  - `Prefers to plan meals...` (default: "Prefers to plan meals…") → **"Multiple days in advance"**
  - `Consults recipes on...` (default: "Consults recipes on…") → **"Tablet / Laptop"**
  - `Consults outside source?` (default: "Consults outside source?"):
    > Yes, consults with his wife & doctor when implementing change, ever since his vitamin deficiency diagnosis.
  - `Pain Points` (default: "	• 	Pain\n	• 	Points\n	• 	For\n	• 	The\n	• 	End\n	• 	User") → **"• Identifying & describing symptoms of deficiency"**

- **Jordan** — master `UX / Persona Template`
  - `Name` (default: "Name") → **"Jordan"**
  - `Occupation` (default: "Occupation") → **"Math Professor"**
  - `Full Name` (default: "Full Name") → **"Jordan Pierce"**
  - `Age` (default: "Age") → **"36"**
  - `Eating Style` (default: "Eating Style") → **"Omnivore"**
  - `Big Text` (default: "  	    Who is the user? \n_.    What is the user’s history w…"):
    >   	    Jordan has made some                 _  attempts at vegan meals; some he liked, some he didn’t. He feels his ability to prepare tofu is improving, but doesn’t like tempeh. 
    >
    > He has tried Purple Carrot, but has never personally veganized a recipe.
    >
    > Jordan tracked macronutrition with the Weight Watchers app, but never micro, beyond “took a multivitamin and hoped it works.” He found the pre-input data from other users was often incorrect, & generic versions of standard meals were often unavailable. Would prefer a more guided/intuitive method of food journaling, plus a reliable databse of generic foods, esp. for eating out.
    >
    > Jordan has watched the Vegify brand develop through its social media presence. He describes his experience as “positive.” He liked the pizza cheese tasteoff, and found it later affected his browsing behavior at the grocery store.
  - `Prefers to plan meals...` (default: "Prefers to plan meals…") → **"Mixture of daily & in advance"**
  - `Consults recipes on...` (default: "Consults recipes on…") → **"Phone (w/ prevent auto-lock ON)"**
  - `Pain Points` (default: "	• 	Pain\n	• 	Points\n	• 	For\n	• 	The\n	• 	End\n	• 	User"):
    > • Availability / accuracy of nutrition information for standard meals
    > • Meal discovery
  - `Consults outside source?` (default: "Consults outside source?") → **"No, but through Vegify is more aware of the need to track things like B12 when eating plant-based."**

- **Simone** — master `UX / Persona Template`
  - `Name` (default: "Name") → **"Simone"**
  - `Occupation` (default: "Occupation") → **"Photographer"**
  - `Full Name` (default: "Full Name") → **"Simone Adler"**
  - `Eating Style` (default: "Eating Style") → **"Plant-based"**
  - `Consults recipes on...` (default: "Consults recipes on…") → **"Phone if simple, otherwise laptop"**
  - `(?C83CB276)`:
    > • Pain 
    > • Points
  - `Age` (default: "Age") → **"28"**
  - `Big Text` (default: "  	    Who is the user? \n_.    What is the user’s history w…"):
    >   	    Simone describes herself as        _     “about 95% dairy-free,” and still eats eggs, but has otherwise eschewed animal products. 
    >
    > She has a lot  of experience veganizing recipes, and knows most of the common substitutions, in addition to having specific preferences depending on the recipe.
    >
    > She has minimal experience tracking nutritional intake, having tried a few calorie tracking apps, but dropped them upon finding they were ill-equipped to accomodate her dietary restrictions.
    >
    > Describes her experience with Vegify as “delicious; stanning since day 1.” She says it has opened her mind to vegan recipes; she loves the videos and their accessible approach to vegan cooking. Simone also co-starred in our vegan cheese tasteoff video. After viewing a video, she frequently shares & leaves a comment.
    >
    >
    >
  - `Pain Points` (default: "	• 	Pain\n	• 	Points\n	• 	For\n	• 	The\n	• 	End\n	• 	User") → **"• Planning around dietary restrictions"**
  - `Prefers to plan meals...` (default: "Prefers to plan meals…") → **"Varies; both daily & in advance"**
  - `Consults outside source?` (default: "Consults outside source?"):
    > Changes her diet as directed, but does not seek specific advice when making dietary choices for herself.

## Page/3.Recipes/2.Add/3.Create or Update Ingredient

### Page/Recipe/Add/Create or Edit Ingredient/Desktop HD

- **UI / Desktop / Nutrition Facts** — master `UI / Desktop / Nutrition Facts`
  - `This Recipe` (default: "This Recipe") → **"This Ingredient"**

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"Recipe"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Add Inredient"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Cals per serving** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Calories Per Serving"**

- **Weight per serving** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Weight per Serving"**

- **Weight** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Package Weight"**

- **Price** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Price"**

- **Food Name** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Food Name"**

### Page/Recipe/Add/Create or Edit Ingredient/Mobile

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Nutrient"**

- **Wireframe/Form Elements/Input/field Copy 4** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"                       unit or %"**

- **Cals per serving** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Calories Per Serving"**

- **Weight per serving** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Weight per Serving"**

- **Weight** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Package Weight"**

- **Price** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Price"**

- **Food Name** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Food Name"**

- **Image** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Ingredient Image"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"Recipe"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Add Ingredient"**

## 🌐 Symbols

### Symbol master: Wireframe/Form Elements/Search Variables

- **Wireframe/Form Elements/Input/field** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"value"**

### Symbol master: Wireframe / Annocation Text

- **Wireframe / Annotation Number Copy** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

## Page/5.Social/1.Profile/0.View Profile

### Page/5.1.0/Social/Profile/View Profile/Desktop HD

- **UI / Desktop / Nutrition Facts** — master `UI / Desktop / Nutrition Facts`
  - `This Recipe` (default: "This Recipe") → **"Today’s Meals"**

- **UI / Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"@username "**

### Page/5.1.0/Social/Profile/View ProfileMobile

- **Cover Photo** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Cover Photo"**

- **UI / Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"@username "**

## Page/3.Recipes/2.0.Create or Edit Recipe

### Page/Recipe/Add or Update/W Ingredient/ Desktop HD

- **Wireframe/Form Elements/Quantity Copy** — master `Wireframe/Form Elements/Quantity / Filled`
  - `10` (default: "10") → **"1"**

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create / Edit Recipe"**

### Page/Recipe/Add or Update/W Ingredient/Mobile

- **Recipe Image** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Recipe Image"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create / Edit Recipe"**

- **Wireframe/Form Elements/Quantity Copy** — master `Wireframe/Form Elements/Quantity / Filled`
  - `10` (default: "10") → **"1"**

### Page/Recipe/Add or Update/Desktop HD

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create / Edit Recipe"**

### Page/Recipe/Add or Update/Mobile

- **Recipe Image** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Recipe Image"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create / Edit Recipe"**

## Sample Profile - Jordan

### Page/5.1.0/Social/Profile/View Profile/Desktop HD

- **UI / Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"@username "**

### Page/5.1.0/Social/Profile/View ProfileMobile

- **Cover Photo** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Cover Photo"**

- **UI / Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"@username "**

## Page/1.Home/1.0.Welcome /0. Logged Out

### Page/1.Home/1.0.Welcome/Annotations

- **3** — master `Wireframe / Annocation Text`
  - `Wireframe / Annotation Number Copy > 000` (default: "000") → **"3"**
  - `Unfinished features` (default: "Unfinished features shall be greyed out") → **"Featured content shall be algorithmically selected from the Vegify database"**

- **2** — master `Wireframe / Annocation Text`
  - `Wireframe / Annotation Number Copy > 000` (default: "000") → **"2"**
  - `Unfinished features` (default: "Unfinished features shall be greyed out"):
    > Clicking “sign up” with credientials shall register the user and take them to Create/Edit Profile page (with those fields already filled in), whereas clicking without credentials shall take the user to Create Profile Page.

- **1** — master `Wireframe / Annocation Text`
  - `Wireframe / Annotation Number Copy > 000` (default: "000") → **"1"**
  - `Unfinished features` (default: "Unfinished features shall be greyed out"):
    > Clicking “Log In” with credentials shall take the user to the home screen (if successful) or display an error (if unsuccessful); clicking “Log In” without credentials displays an error. 

### Page/1.Home/1.0.Welcome/Desktop HD/Base

- **Wireframe/Form Elements/Input/field Copy** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Password"**

- **Wireframe/Form Elements/Input/field** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Username or email"**

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Home "**

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

### Page/1.Home/1.0.Welcome/Desktop HD/Error

- **Wireframe/Form Elements/Input/field Copy** — master `Wireframe/Form Elements/Input/Error`
  - `Input field` (default: "Input field") → **"Password"**

- **Wireframe/Form Elements/Input/field** — master `Wireframe/Form Elements/Input/Error`
  - `Input field` (default: "Input field") → **"Username or email"**

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Home "**

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

### Page/1.Home/1.0.Welcome/Desktop HD/Filled In

- **Wireframe/Form Elements/Input/field Copy** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"************"**

- **Wireframe/Form Elements/Input/field** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"dev@example.com"**

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Home "**

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

### Page/1.Home/1.2.Welcome/Mobile/Filled In

- **UI / Sign Up button** — master `UI / Sign Up button`
  - `Sign Up` (default: "Sign Up") → **"Log In / Sign Up"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs "):
    > Home
    >

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

- **Wireframe/Form Elements/Input/field Copy 2** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"**********"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"dev@example.com"**

### Page/1.Home/1.1.Welcome/Mobile/Error

- **UI / Sign Up button** — master `UI / Sign Up button`
  - `Sign Up` (default: "Sign Up") → **"Log In / Sign Up"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs "):
    > Home
    >

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

- **Wireframe/Form Elements/Input/field Copy 2** — master `Wireframe/Form Elements/Input/Error`
  - `Input field` (default: "Input field") → **"Password"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Error`
  - `Input field` (default: "Input field") → **"Username or email"**

### Page/1.Home/1.0.Welcome/Mobile/Base

- **UI / Sign Up button** — master `UI / Sign Up button`
  - `Sign Up` (default: "Sign Up") → **"Log In / Sign Up"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs "):
    > Home
    >

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

- **Wireframe/Form Elements/Input/field Copy 2** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Password"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Username or email"**

## Page/3.Recipes/2.Add/3.Ingredient/2.Results

### Page/Recipes/Add/Search Results/Desktop HD

- **Search Box** — master `UI / Desktop / Search Box`
  - `Search Text` (default: "Search…") → **"Black Beans"**

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"Recipe"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Add Ingredient"**

### Page/Recipes/Add/Search Results/Mobile

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"Recipe"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Add Ingredient"**

- **UI / Mobile / Search Box** — master `UI / Mobile / Search Box`
  - `Search Text` (default: "Search…") → **"Black Beans"**

## xxx Page/1.Home/3.0.Log In or Sign Up

### Symbol master: Page/1.Home/3.0.Log In or Sign Up/Desktop HD

- **Wireframe/Form Elements/Input/field Copy** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Password"**

- **Wireframe/Form Elements/Input/field** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Username or email"**

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Home "**

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

### Symbol master: Page/1.Home/3.0.Log In or Sign Up/Mobile

- **UI / Sign Up button** — master `UI / Sign Up button`
  - `Sign Up` (default: "Sign Up") → **"Log In / Sign Up"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs "):
    > Home
    >

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

- **Wireframe/Form Elements/Input/field Copy 2** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Password"**

- **Wireframe/Form Elements/Input/field Copy 3** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Username or email"**

## Page/1.Home/1.0.Welcome /4. Newsfeed

### Page/1.Home/1.Welcome/4.Newsfeed/Annotations

- **3** — master `Wireframe / Annocation Text`
  - `Wireframe / Annotation Number Copy > 000` (default: "000") → **"3"**
  - `Unfinished features` (default: "Unfinished features shall be greyed out") → **"Featured content shall be algorithmically selected from the Vegify database"**

- **2** — master `Wireframe / Annocation Text`
  - `Wireframe / Annotation Number Copy > 000` (default: "000") → **"2"**
  - `Unfinished features` (default: "Unfinished features shall be greyed out"):
    > Clicking “sign up” with credientials shall register the user and take them to Create/Edit Profile page (with those fields already filled in), whereas clicking without credentials shall take the user to Create Profile Page.

- **1** — master `Wireframe / Annocation Text`
  - `Wireframe / Annotation Number Copy > 000` (default: "000") → **"1"**
  - `Unfinished features` (default: "Unfinished features shall be greyed out"):
    > Clicking “Log In” with credentials shall take the user to the home screen (if successful) or display an error (if unsuccessful); clicking “Log In” without credentials displays an error. 

### Page/1.Home/1.Welcome/4.Newsfeed/Desktop HD

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Home "**

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

### Page/1.Home/1.Welcome/4.Newsfeed/Mobile

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs "):
    > Home
    >

- **3** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"3"**

- **2** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"2"**

- **1** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"1"**

- **0** — master `Wireframe / Annotation Number`
  - `000` (default: "000") → **"0"**

## Page/2.Search/1.0.Search

### Page/Search/Desktop HD

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Search Results"**

- **Search Box** — master `UI / Desktop / Search Box`
  - `Search Text` (default: "Search…") → **"Search..."**

### Page/Search/Mobile

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Search Results"**

### Page/Search/Mobile/Filter

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Search Results"**

- **UI / Mobile / Search Box** — master `UI / Mobile / Search Box`
  - `Search Text` (default: "Search…") → **"Search..."**

## User Flows

### Site Flow

- **UI Template Mobile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Welcome Screen"**

- **Node/Comment Copy 2** — master `Node/Comment`
  - `Text` (default: "Text") → **"Register Button"**

- **Node/Decision** — master `Node/Decision`
  - `Text` (default: "Text"):
    > NEW
    > USER?

- **Node/Decision Copy** — master `Node/Decision`
  - `Text` (default: "Text"):
    > Profile
    > Successfully
    > Created

- **UI Template Mobile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"How to Meal Plan"**

- **UI Template Mobile Copy** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Register: Credentials"**

- **UI Template Mobile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Settings/About/Patreon Link"**

- **UI Template Mobile Copy 3** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Login"**

- **UI Template Mobile Copy 4** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Success Confirmation"**

- **UI Template Mobile Copy 2** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Register: Calculate Nutritional Needs"**

- **Node/Comment Copy** — master `Node/Comment`
  - `Text` (default: "Text") → **"No"**

- **Node/Rectangle** — master `Node/Rectangle`
  - `Text` (default: "Text") → **"Quick walkthrough of how to plan for complete nutrition, concluding with “Register” button"**

- **Node/Rectangle Copy** — master `Node/Rectangle`
  - `Text` (default: "Text"):
    > Search results can be
    > people, recipes, meal plans, or ingrdients.
    > Separated by tabs

- **Node/Rectangle Copy 3** — master `Node/Rectangle`
  - `Text` (default: "Text"):
    > Notifications are for when someone “faves” or comments on your recipe or meal plan. Clicking notifications takes you to the page in question, clicking the profile image takes you to that user’s page.

- **Node/Comment** — master `Node/Comment`
  - `Text` (default: "Text") → **"Yes"**

- **Home | Newsfeed** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Home -  Recipes and Meal Plans From People You Follow"**

- **UI Template Mobile Copy 6** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"View Recipe"**

- **UI Template Mobile Copy 6** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Meal Plan List View"**

- **UI Template Mobile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title"):
    > Meal Plan
    > Calendar View

- **Nutritional Info** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Nutritional Info"**

- **UI Template Mobile Copy 9** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Share "**

- **Search** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Search"**

- **Search Copy** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Search"**

- **Search Results** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Search Results"**

- **Notifications** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Notifications"**

- **Notifications Copy** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Following / Followers"**

- **Inbox** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Inbox"**

- **Inbox Copy** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Message"**

- **My Profile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title"):
    > Profile
    > My Recipes
    > My Meal Plans

- **Node/Comment Copy 4** — master `Node/Comment`
  - `Text` (default: "Text") → **"Tap “Save Recipe”"**

- **Node/Comment Copy 3** — master `Node/Comment`
  - `Text` (default: "Text") → **"Tap on Recipe"**

- **Node/Comment Copy 6** — master `Node/Comment`
  - `Text` (default: "Text"):
    > Type
    > Query

- **Node/Comment Copy 5** — master `Node/Comment`
  - `Text` (default: "Text") → **"Tap on Meal Plan"**

- **Node / Tap Save Meal Plan** — master `Node/Comment`
  - `Text` (default: "Text") → **"Tap “Save Meal Plan”"**

- **Node / Tap Message** — master `Node/Comment`
  - `Text` (default: "Text"):
    > Tap
    > Message

- **Node/Comment Copy 2** — master `Node/Comment`
  - `Text` (default: "Text") → **"Tap “Save”"**

- **UI Template Mobile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Create / Edit Recipe or Meal Plan"**

- **Node/Decision** — master `Node/Decision`
  - `Text` (default: "Text"):
    > Successfully
    > Saved

- **Node/Comment Copy 9** — master `Node/Comment`
  - `Text` (default: "Text") → **"Tap “Add Ingredient” "**

- **Node/Comment Copy 11** — master `Node/Comment`
  - `Text` (default: "Text") → **"Find Ingredient & Tap “Customize”"**

- **UI Template Mobile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title") → **"Create / Edit Ingredient"**

- **Node/Comment Copy 10** — master `Node/Comment`
  - `Text` (default: "Text") → **"Find Ingredient & Tap “Add to Recipe”"**

- **Node/Decision Copy 2** — master `Node/Decision`
  - `Text` (default: "Text") → **"Ingredient Not Found / Add ingredient"**

- **Node/Comment Copy 2** — master `Node/Comment`
  - `Text` (default: "Text") → **"Tap “Save”"**

- **Node/Decision** — master `Node/Decision`
  - `Text` (default: "Text"):
    > Successfully
    > Added to Recipe

- **Node/Decision Copy 3** — master `Node/Decision`
  - `Text` (default: "Text"):
    > Successfully
    > Saved Ingredient

- **UI Template Mobile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title"):
    > Scan Label
    > (Ingredient Only)

- **UI Template Mobile** — master `⚛️/ UI Template Mobile`
  - `Title` (default: "Title"):
    > Source?
    > Faves, Search, Scan

## Page/3.Recipes/1.0.View Recipe

### Page/3.1.0/Recipes/View Recipe/Desktop HD

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Recipe Name"**

### Page/3.1.0/Recipes/View Recipe/Mobile/Nutrition

- **Recipe Image** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Recipe Image"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Recipe Name"**

### Page/3.1.0/Recipes/View Recipe/Mobile/Base

- **Recipe Image** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Recipe Image"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Recipe Name"**

### Page/3.1.0/Recipes/View Recipe/Mobile/Base Gold

- **Recipe Image** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Recipe Image"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Recipe Name"**

### Page/3.1.0/Recipes/View Recipe/Mobile/Base Dark

- **Recipe Image** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Recipe Image"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Recipe Name"**

## Page/3.Recipes/2.Add/2.Scan Label

### Page/Recipes/Add/Search for Ingredient/Desktop HD

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create / Edit Recipe"**

### Page/Recipes/Add/Search for Ingredient/Mobile

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create / Edit Recipe"**

## Page/3.Recipes/3.0.View Ingredient

### View Ingredient / Desktop HD

- **UI / Desktop / Nutrition Facts** — master `UI / Desktop / Nutrition Facts`
  - `This Recipe` (default: "This Recipe") → **"This Ingredient"**

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Recipe Name"**

### View Ingredient / Mobile

- **Recipe Image** — master `Wireframe/Page Elements/Image`
  - `Thumbnail` (default: "Thumbnail\n") → **"Recipe Image"**

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"@user"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Recipe Name"**

## Page/1.Home/3.0.Add or Update Profile

### Page/Social/Create or Update/Desktop HD

- **Weight** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Weight"**

- **Height** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Height"**

- ***password** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"*password"**

- ***email** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"*email"**

- **Bio** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Bio…"**

- ***username** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"*username"**

- **UI / Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create/Edit Profile "**

### Page/Social/Create or Update/Mobile

- **Weight** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Weight"**

- **Height** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Height"**

- **Bio...** — master `Wireframe/Form Elements/Input/Base`
  - `Input field` (default: "Input field") → **"Bio…"**

- ***password** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"*password"**

- ***username** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"*username"**

- ***email** — master `Wireframe/Form Elements/Input/Filled`
  - `Input field` (default: "Input field") → **"*email"**

- **UI / Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Create/Edit Profile "**

## Page/2.Search/1.1.Results

### Page/Search/Results/Desktop HD

- **Breadcrumbs** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Search Results"**

### Page/Search/Results/Mobile

- **Breadcrumbs Copy** — master `UI / Breadcrumbs`
  - `Home` (default: "Home") → **"vegify"**
  - `Breadcrumbs` (default: "Breadcrumbs ") → **"Search Results"**

