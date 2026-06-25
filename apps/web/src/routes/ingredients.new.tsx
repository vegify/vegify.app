import { createFileRoute, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { useQueryClient } from '@tanstack/react-query'
import { IngredientForm, type IngredientFormInput } from '@vegify/ui'

const saveIngredientFn = createServerFn({ method: 'POST' })
  .validator((input: IngredientFormInput) => input)
  .handler(async ({ data }) => {
    const { saveIngredient } = await import('../content')
    return saveIngredient(data)
  })

export const Route = createFileRoute('/ingredients/new')({
  component: NewIngredient,
})

function NewIngredient() {
  const router = useRouter()
  const queryClient = useQueryClient()
  return (
    <IngredientForm
      onSave={async (input) => {
        const id = await saveIngredientFn({ data: input })
        await queryClient.invalidateQueries({ queryKey: ['ingredients'] }) // list gains the new item
        router.navigate({
          to: '/ingredients/$ingredientId',
          params: { ingredientId: String(id) },
        })
      }}
    />
  )
}
