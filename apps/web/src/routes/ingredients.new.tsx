import { createFileRoute, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { IngredientForm, type IngredientFormInput } from '@vegify/ui'

const saveIngredientFn = createServerFn({ method: 'POST' })
  .validator((input: IngredientFormInput) => input)
  .handler(async ({ data }) => {
    const { saveIngredient } = await import('@vegify/db')
    const { currentUserId } = await import('../auth')
    return saveIngredient({ ...data, userId: await currentUserId() })
  })

export const Route = createFileRoute('/ingredients/new')({
  component: NewIngredient,
})

function NewIngredient() {
  const router = useRouter()
  return (
    <IngredientForm
      onSave={async (input) => {
        const id = await saveIngredientFn({ data: input })
        router.navigate({
          to: '/ingredients/$ingredientId',
          params: { ingredientId: String(id) },
        })
      }}
    />
  )
}
