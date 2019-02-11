local Rojo = script:FindFirstAncestor("Rojo")

local Roact = require(Rojo.Roact)

local e = Roact.createElement

local function DialogBackdrop(props)
	return e("ImageButton", {
		Size = UDim2.new(1, 0, 1, 0),
		BackgroundColor3 = Color3.new(0, 0, 0),
		BackgroundTransparency = 0.5,
		BorderSizePixel = 0,
		AutoButtonColor = false,

		[Roact.Event.Activated] = function()
			if props.onClose ~= nil then
				props.onClose()
			end
		end,
	}, props[Roact.Children])
end

return DialogBackdrop